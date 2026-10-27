package ro.novabytecode.mira.companion

import android.app.Activity
import android.content.Intent
import com.facebook.react.bridge.*
import com.journeyapps.barcodescanner.ScanOptions
import com.journeyapps.barcodescanner.ScanContract

/** In-app live camera preview with autofocus and automatic QR decoding. */
class PairingScanner(private val context: ReactApplicationContext) : BaseActivityEventListener() {
    private var pending: Promise? = null
    init { context.addActivityEventListener(this) }
    fun scan(promise: Promise) {
        if (pending != null) { promise.reject("QR", "Scanare deja pornită."); return }
        val activity = context.currentActivity ?: run { promise.reject("QR", "Deschide aplicația."); return }
        try {
            val options = ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Îndreaptă camera spre codul QR Mira de pe PC. Scanarea este automată.")
                .setBeepEnabled(false)
                .setBarcodeImageEnabled(false)
                .setOrientationLocked(false)
                .setTimeout(60000L)
            pending = promise
            activity.startActivityForResult(ScanContract().createIntent(activity, options), 904)
        } catch (e: Exception) {
            pending = null
            promise.reject("QR", "Camera nu este disponibilă. Poți lipi codul de asociere.", e)
        }
    }
    override fun onActivityResult(activity: Activity, requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode != 904) return
        val promise = pending ?: return
        pending = null
        val code = ScanContract().parseResult(resultCode, data).contents
        if (code == null) { promise.reject("QR", "Scanare oprită sau accesul la cameră nu a fost permis."); return }
        if (!code.startsWith("mira-pair://")) { promise.reject("QR", "Acesta nu este un cod de asociere Mira."); return }
        promise.resolve(code)
    }
}
