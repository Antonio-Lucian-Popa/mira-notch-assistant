package ro.novabytecode.mira.companion
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
@RunWith(AndroidJUnit4::class)
class VoiceWorkflowTest {
    @Test fun exportPersistsUnicodeAndOptionallyTransfersToThePinnedDesktop(){
        val instrumentation=InstrumentationRegistry.getInstrumentation();val context=instrumentation.targetContext
        val store=TranscriptStore(context);val row=store.create();val id=row.getString("id")
        try{
            val text=(1..30).joinToString("\n\n"){"Punctul $it. Astăzi am discutat despre proiectul Mira. Următorii pași: ședință, căutare, încărcare și verificarea conexiunii securizate."}
            row.put("text",text).put("complete",true).put("title","Idei și pași următori");store.save(row)
            val pdf=TranscriptPdf.export(context,row);assertTrue(pdf.readBytes().take(5).toByteArray().toString(Charsets.US_ASCII)=="%PDF-")
            val original=pdf.readBytes();assertArrayEquals(original,TranscriptPdf.export(context,row).readBytes())
            pdf.copyTo(File(context.getExternalFilesDir(null),"mira-transcript-preview.pdf"),overwrite=true)
            assertEquals(text,store.get(id).getString("text"));assertFalse(store.list().getJSONObject(0).has("text"))
            val code=androidx.test.platform.app.InstrumentationRegistry.getArguments().getString("pairing")
            if(!code.isNullOrBlank()){
                TranscriptSync.disconnect(context)
                val wrong=code.replace(Regex("fp=[a-f0-9]{64}"),"fp="+"0".repeat(64))
                try{TranscriptSync.pair(context,wrong);fail("Wrong certificate accepted")}catch(_:javax.net.ssl.SSLException){}
                assertFalse(TranscriptSync.status(context).getBoolean("paired"))
                TranscriptSync.pair(context,code);assertTrue(TranscriptSync.upload(context));assertTrue(store.get(id).getBoolean("synced"))
                val actions=TranscriptSync.actions(context);assertTrue(actions.length()>=1)
                // Do not send real lock/keyboard actions to the user's desktop from tests.
            }
        }finally{TranscriptSync.disconnect(context);store.delete(id)}
    }
}
