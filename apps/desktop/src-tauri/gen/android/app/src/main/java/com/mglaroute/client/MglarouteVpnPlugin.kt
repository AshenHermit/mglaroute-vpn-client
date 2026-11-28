package com.mglaroute.client

import android.Manifest
import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.VpnService
import android.os.Build
import android.os.IBinder
import android.webkit.WebView
import java.io.File
import java.io.FileOutputStream
import androidx.activity.ActivityResultContracts
import androidx.activity.result.ActivityResultLauncher
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class VpnStartArgs {
    lateinit var address: String
    lateinit var dns: String
    var routes: List<String> = emptyList()
    var disallowedPackages: List<String> = emptyList()
}

@InvokeArg
class StageAssetsArgs {
    /** Absolute path to the app-writable binaries dir (filesDir/binaries). */
    lateinit var targetDir: String
}

/** Bridge between Tauri's mobile plugin API and [MglarouteVpnService]. */
@TauriPlugin(
    permissions = [
        Permission(
            strings = [Manifest.permission.POST_NOTIFICATIONS],
            alias = "postNotifications",
        ),
    ],
)
class MglarouteVpnPlugin(private val activity: Activity) : Plugin(activity) {
    private var vpnPermissionLauncher: ActivityResultLauncher<Intent>? = null
    private var pendingPrepare: Invoke? = null

    override fun load(webView: WebView) {
        // Kotlin owns APK assets; stage them into the app files dir here so the
        // Rust launcher can resolve real filesystem paths before first connect.
        stageAssets(File(activity.filesDir, "binaries"))
        // Register before any prepare() call: launching the consent activity
        // needs a callback that already exists when the result comes back.
        vpnPermissionLauncher = (activity as AppCompatActivity).registerForActivityResult(
            ActivityResultContracts.StartActivityForResult(),
        ) { result ->
            val granted = result.resultCode == Activity.RESULT_OK
            val response = JSObject()
            response.put("granted", granted)
            pendingPrepare?.resolve(response)
            pendingPrepare = null
        }
        if (
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(
                activity,
                Manifest.permission.POST_NOTIFICATIONS,
            ) != PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(
                activity,
                arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                NOTIFICATION_REQUEST_CODE,
            )
        }
    }

    /**
     * Tauri mobile bundles `bundle.resources` as APK assets, not filesystem
     * paths. Copy xray and the geo dat files into a real app-writable dir and
     * make the executables chmod +x before Rust tries to spawn them.
     */
    @Command
    fun stage_assets(invoke: Invoke) {
        val args = try {
            invoke.parseArgs(StageAssetsArgs::class.java)
        } catch (_: Exception) {
            invoke.reject("invalid asset staging request", "asset_staging_failed")
            return
        }
        if (stageAssets(File(args.targetDir))) invoke.resolve()
        else invoke.reject("cannot stage bundled binaries", "asset_staging_failed")
    }

    /** Copy APK assets into a real app-writable dir; xray gets +x. */
    private fun stageAssets(target: File): Boolean {
        return try {
            if (!target.isDirectory && !target.mkdirs()) return false
            for (asset in STAGED_ASSETS) {
                val destination = File(target, asset.substringAfterLast('/'))
                activity.assets.open(asset).use { input ->
                    FileOutputStream(destination).use { output ->
                        val buffer = ByteArray(16 * 1024)
                        while (true) {
                            val read = input.read(buffer)
                            if (read < 0) break
                            output.write(buffer, 0, read)
                        }
                    }
                }
                if (asset.contains("xray-") && !destination.setExecutable(true, false)) {
                    return false
                }
            }
            true
        } catch (_: Exception) {
            false
        }
    }

    @Command
    fun prepare(invoke: Invoke) {
        val consentIntent = VpnService.prepare(activity)
        if (consentIntent == null) {
            val response = JSObject()
            response.put("granted", true)
            invoke.resolve(response)
            return
        }
        val launcher = vpnPermissionLauncher
        if (launcher == null) {
            invoke.reject("VPN consent callback is not registered", "permission_required")
            return
        }
        pendingPrepare?.reject("VPN consent request was superseded", "permission_required")
        pendingPrepare = invoke
        launcher.launch(consentIntent)
    }

    @Command
    fun start(invoke: Invoke) {
        val args = try {
            invoke.parseArgs(VpnStartArgs::class.java)
        } catch (_: Exception) {
            invoke.reject("invalid VPN start request", "start_failed")
            return
        }
        val running = MglarouteVpnService.active
        if (running != null) {
            completeStart(invoke, running, args)
            return
        }
        val intent = Intent(activity, MglarouteVpnService::class.java)
        val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName?, service: IBinder) {
                try {
                    val vpnService = (service as MglarouteVpnService.LocalBinder).service()
                    completeStart(invoke, vpnService, args)
                } finally {
                    activity.unbindService(this)
                }
            }

            override fun onServiceDisconnected(name: ComponentName?) = Unit
        }
        try {
            activity.startForegroundService(intent)
            if (!activity.bindService(intent, connection, Context.BIND_AUTO_CREATE)) {
                stopActiveSessionSafely()
                invoke.reject("VPN service is unavailable", "interface_unavailable")
            }
        } catch (_: SecurityException) {
            stopActiveSessionSafely()
            invoke.reject("VPN permission is required", "permission_required")
        }
    }

    @Command
    fun stop(invoke: Invoke) {
        val service = MglarouteVpnService.active
        if (service != null) service.stopSession()
        val event = JSObject()
        event.put("established", false)
        trigger("tun-established", event)
        invoke.resolve()
    }

    @Command
    fun interface_name(invoke: Invoke) {
        val name = interfaceName()
        if (name == null) {
            invoke.reject("VPN interface is unavailable", "interface_unavailable")
            return
        }
        val response = JSObject()
        response.put("interfaceName", name)
        invoke.resolve(response)
    }

    @Command
    fun wait_established(invoke: Invoke) {
        val response = JSObject()
        response.put("established", MglarouteVpnService.active?.isEstablished() == true)
        invoke.resolve(response)
    }

    /** Physical-network DNS servers as IP literals; never the virtual VPN DNS. */
    @Command
    fun dns_servers(invoke: Invoke) {
        val manager = activity.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
        val servers = manager?.let {
            it.getLinkProperties(it.activeNetwork)?.dnsServers?.mapNotNull { address ->
                address.hostAddress
            }
        }.orEmpty().filter { it.isNotBlank() }
        if (servers.isEmpty()) {
            invoke.reject("DNS servers are unavailable", "dns_unavailable")
            return
        }
        val response = JSObject()
        response.put("dnsServers", servers)
        invoke.resolve(response)
    }

    private fun completeStart(invoke: Invoke, service: MglarouteVpnService, args: VpnStartArgs) {
        val fd = service.startSession(args)
        if (fd == null) {
            stopActiveSessionSafely()
            invoke.reject("VPN interface is unavailable", "interface_unavailable")
            return
        }
        val name = interfaceName()
        if (name == null) {
            // No handoff happened, so this descriptor is still ours to close.
            try {
                android.system.Os.close(fd)
            } catch (_: Exception) {
                // Closing is best effort; teardown must still happen.
            }
            stopActiveSessionSafely()
            invoke.reject("VPN interface is unavailable", "interface_unavailable")
            return
        }
        val response = JSObject()
        response.put("fd", fd)
        response.put("interfaceName", name)
        trigger("tun-established", JSObject().put("established", true))
        invoke.resolve(response)
    }

    /**
     * A foreground service started before `establish()` may still be running
     * when `start` fails; Rust cannot see a session flag it was never told
     * about, so Kotlin owns this defensive teardown.
     */
    private fun stopActiveSessionSafely() {
        try {
            MglarouteVpnService.active?.stopSession()
        } catch (_: Exception) {
            // Never mask the original start failure with a teardown error.
        }
    }

    private fun interfaceName(): String? {
        val manager = activity.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            ?: return null
        return manager.getLinkProperties(manager.activeNetwork)?.interfaceName
    }

    private companion object {
        const val NOTIFICATION_REQUEST_CODE = 4102
        val STAGED_ASSETS = listOf(
            "binaries/xray-aarch64-linux-android",
            "binaries/xray-x86_64-linux-android",
            "binaries/geoip.dat",
            "binaries/geosite.dat",
        )
    }
}
