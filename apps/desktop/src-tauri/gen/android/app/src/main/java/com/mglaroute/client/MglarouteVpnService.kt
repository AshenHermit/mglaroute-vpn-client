package com.mglaroute.client

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.net.VpnService
import android.os.Binder
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

/**
 * Owns the Android VPN interface for one session. The TUN descriptor is
 * detached and handed to the xray core by the Rust side; no subscription URL,
 * credential or raw config is ever placed in a log or an intent extra.
 */
class MglarouteVpnService : VpnService() {
    inner class LocalBinder : Binder() {
        fun service(): MglarouteVpnService = this@MglarouteVpnService
    }

    private val binder = LocalBinder()
    private var hasSession = false

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        startForeground(NOTIFICATION_ID, notification())
        active = this
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // The interface is created by startSession() once the plugin binds.
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? =
        // The system binds with the VPN service action; keep the platform
        // binder for that path and expose the local binder only to our plugin.
        if (intent?.action == "android.net.VpnService") super.onBind(intent) else binder

    override fun onDestroy() {
        // Drop plugin-visible session state; the fd is owned by the core after
        // handoff and is deliberately not closed here.
        hasSession = false
        active = null
        stopForeground(STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }

    /** Returns the detached TUN fd, or null when the interface could not be created. */
    fun startSession(args: VpnStartArgs): Int? {
        // Foreground service must be live before a blocking VpnService is built.
        startForeground(NOTIFICATION_ID, notification())

        val builder = Builder()
            .setSession(SESSION_NAME)
            .setBlocking(true)
        if (args.address.isBlank()) {
            builder.addAddress("10.10.0.1/30")
            builder.addDnsServer("10.10.0.2")
        } else {
            builder.addAddress(args.address)
            builder.addDnsServer(args.dns)
        }
        val routes = if (args.routes.isEmpty()) listOf("0.0.0.0/0") else args.routes
        for (route in routes) {
            builder.addRoute(route.removePrefix("ip:"))
        }
        // Exclude the app itself so VPN traffic cannot loop back into the core.
        builder.addDisallowedApplication(packageName)
        for (pkg in args.disallowedPackages) {
            if (pkg != packageName) builder.addDisallowedApplication(pkg)
        }

        val parcelFd = try {
            builder.establish()
        } catch (_: SecurityException) {
            null
        } ?: return null
        hasSession = true
        return parcelFd.detachFd()
    }

    fun stopSession() {
        hasSession = false
        stopSelf()
    }

    fun isEstablished(): Boolean = hasSession

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            CHANNEL_ID,
            "VPN connection",
            NotificationManager.IMPORTANCE_LOW,
        )
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun notification(): Notification {
        val intent = Intent(this, MainActivity::class.java)
        val pendingIntent = PendingIntent.getActivity(
            this,
            0,
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.stat_sys_vpn_ic)
            .setContentTitle("mglaroute VPN")
            .setOngoing(true)
            .setContentIntent(pendingIntent)
            .build()
    }

    companion object {
        private const val CHANNEL_ID = "mglaroute-vpn"
        private const val NOTIFICATION_ID = 4101
        private const val SESSION_NAME = "mglaroute"

        @Volatile
        var active: MglarouteVpnService? = null
            private set
    }
}
