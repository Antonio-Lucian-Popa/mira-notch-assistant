package ro.novabytecode.mira.companion

import android.content.ComponentName
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Queries engine capabilities only; never opens the microphone or changes user settings. */
@RunWith(AndroidJUnit4::class)
class VoiceCapabilitiesTest {
    @Test fun reportInstalledRomanianRecognitionSupport() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val report = JSONObject().put("sdk", Build.VERSION.SDK_INT)
        val results = JSONArray()
        if (Build.VERSION.SDK_INT >= 33) {
            val configured = Settings.Secure.getString(context.contentResolver, "voice_recognition_service")
            report.put("configuredProvider", configured)
            for (onDevice in listOf(true, false)) {
                for (language in listOf("ro-RO", "ro")) {
                    val result = JSONObject().put("onDeviceApi", onDevice).put("requestedLanguage", language)
                    val done = CountDownLatch(1)
                    var recognizer: SpeechRecognizer? = null
                    var delivered = false
                    instrumentation.runOnMainSync {
                        try {
                            recognizer = if (onDevice) SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
                                else SpeechRecognizer.createSpeechRecognizer(context, ComponentName.unflattenFromString(configured)!!)
                            val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                                .putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
                            recognizer!!.checkRecognitionSupport(intent, context.mainExecutor, object : RecognitionSupportCallback {
                                override fun onSupportResult(support: RecognitionSupport) {
                                    if (delivered) return
                                    delivered = true
                                    result.put("installed", JSONArray(support.installedOnDeviceLanguages))
                                        .put("supportedOffline", JSONArray(support.supportedOnDeviceLanguages))
                                        .put("pending", JSONArray(support.pendingOnDeviceLanguages))
                                        .put("online", JSONArray(support.onlineLanguages))
                                    done.countDown()
                                }
                                override fun onError(error: Int) { if (!delivered) { delivered = true; result.put("error", error); done.countDown() } }
                            })
                        } catch (error: Exception) { result.put("exception", error.javaClass.simpleName); done.countDown() }
                    }
                    if (!done.await(8, TimeUnit.SECONDS)) result.put("timeout", true)
                    instrumentation.runOnMainSync { recognizer?.destroy() }
                    results.put(result)
                }
            }
        }
        report.put("results", results)
        val output = File(context.getExternalFilesDir(null), "voice-capabilities.json")
        output.writeText(report.toString(2))
        assertTrue(output.isFile)
    }
}
