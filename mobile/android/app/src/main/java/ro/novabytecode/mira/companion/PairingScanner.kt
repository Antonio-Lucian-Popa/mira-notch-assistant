package ro.novabytecode.mira.companion
import android.app.Activity
import android.content.Intent
import android.graphics.BitmapFactory
import android.provider.MediaStore
import androidx.core.content.FileProvider
import com.facebook.react.bridge.*
import com.google.zxing.*
import com.google.zxing.common.HybridBinarizer
import java.io.File
class PairingScanner(private val context:ReactApplicationContext):BaseActivityEventListener(){
    private var pending:Promise?=null
    private var file:File?=null
    init{context.addActivityEventListener(this)}
    fun scan(promise:Promise){
        if(pending!=null){promise.reject("QR","Scanare deja pornită.");return}
        val activity=context.currentActivity?:run{promise.reject("QR","Deschide aplicația.");return}
        try{
            file=File(context.cacheDir,"qr").apply{mkdirs()}.let{File(it,"capture.jpg")}
            val uri=FileProvider.getUriForFile(context,context.packageName+".files",file!!)
            pending=promise;activity.startActivityForResult(Intent(MediaStore.ACTION_IMAGE_CAPTURE).putExtra(MediaStore.EXTRA_OUTPUT,uri).addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION),904)
        }catch(e:Exception){pending=null;promise.reject("QR","Camera nu este disponibilă. Poți lipi codul de asociere.",e)}
    }
    override fun onActivityResult(activity:Activity,requestCode:Int,resultCode:Int,data:Intent?){
        if(requestCode!=904)return
        val promise=pending?:return;pending=null;val captured=file?:return
        if(resultCode!=Activity.RESULT_OK){captured.delete();promise.reject("QR","Scanare anulată.");return}
        Thread{
            try{
                val bounds=BitmapFactory.Options().apply{inJustDecodeBounds=true};BitmapFactory.decodeFile(captured.path,bounds)
                val options=BitmapFactory.Options().apply{inSampleSize=1};while(bounds.outWidth/options.inSampleSize>1800||bounds.outHeight/options.inSampleSize>1800)options.inSampleSize*=2
                val bitmap=BitmapFactory.decodeFile(captured.path,options)?:error("Nu pot citi fotografia.")
                val pixels=IntArray(bitmap.width*bitmap.height);bitmap.getPixels(pixels,0,bitmap.width,0,0,bitmap.width,bitmap.height)
                val result=MultiFormatReader().decode(BinaryBitmap(HybridBinarizer(RGBLuminanceSource(bitmap.width,bitmap.height,pixels))),mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),DecodeHintType.TRY_HARDER to true))
                bitmap.recycle();require(result.text.startsWith("mira-pair://")){"Acesta nu este un cod Mira."};promise.resolve(result.text)
            }catch(e:Exception){promise.reject("QR","Codul nu a putut fi citit. Fotografiază-l mai aproape sau lipește codul.",e)}finally{captured.delete()}
        }.start()
    }
}
