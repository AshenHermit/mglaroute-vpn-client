package com.mglaroute.client

import android.content.Intent
import androidx.activity.result.ActivityResultLauncher

/** VPN consent launcher created by MainActivity before its lifecycle reaches STARTED. */
object MglarouteVpnConsent {
    var launcher: ActivityResultLauncher<Intent>? = null
    var onResult: ((Boolean) -> Unit)? = null

    fun launch(intent: Intent, onResult: (Boolean) -> Unit) {
        this.onResult = onResult
        val active = launcher
        if (active == null) {
            this.onResult = null
            onResult(false)
            return
        }
        active.launch(intent)
    }
}
