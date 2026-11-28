package com.mglaroute.client

import android.app.Activity
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    MglarouteVpnConsent.launcher = registerForActivityResult(
      ActivityResultContracts.StartActivityForResult(),
    ) { result ->
      MglarouteVpnConsent.onResult?.invoke(result.resultCode == Activity.RESULT_OK)
    }
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
