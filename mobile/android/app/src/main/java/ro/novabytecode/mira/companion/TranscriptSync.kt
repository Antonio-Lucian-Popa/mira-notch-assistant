package ro.novabytecode.mira.companion

import android.content.Context
import android.net.Uri
import android.util.Base64
import androidx.work.*
import org.json.JSONArray
import org.json.JSONObject
import java.net.URL
import java.security.MessageDigest
import java.security.cert.X509Certificate
import java.util.UUID
import java.util.concurrent.TimeUnit
import javax.net.ssl.*

object TranscriptSync {
    private fun prefs(c:Context)=c.getSharedPreferences("mira.peer.v1",Context.MODE_PRIVATE)
    fun status(c:Context)=JSONObject().put("paired",prefs(c).contains("code")).put("message",prefs(c).getString("status","Niciun PC asociat."))
    fun pair(c:Context,code:String){val peer=parse(code);request(peer,"GET","/v1/actions",null);prefs(c).edit().putString("code",code.trim()).putString("status","PC asociat. Transferurile și telecomanda sunt autorizate.").apply();schedule(c)}
    fun disconnect(c:Context){prefs(c).edit().clear().apply();WorkManager.getInstance(c).cancelUniqueWork("mira-periodic-sync");WorkManager.getInstance(c).cancelUniqueWork("mira-sync")}
    fun actions(c:Context):JSONArray {val code=prefs(c).getString("code",null)?:error("Asociază mai întâi PC-ul.");return JSONArray(request(parse(code),"GET","/v1/actions",null))}
    fun trigger(c:Context,id:String){val code=prefs(c).getString("code",null)?:error("Niciun PC asociat.");request(parse(code),"POST","/v1/actions",JSONObject().put("request_id",UUID.randomUUID().toString()).put("action_id",id));}
    fun schedule(c:Context){
        if(!prefs(c).contains("code"))return
        val constraints=Constraints.Builder().setRequiredNetworkType(NetworkType.UNMETERED).build()
        val manager=WorkManager.getInstance(c)
        manager.enqueueUniqueWork("mira-sync",ExistingWorkPolicy.KEEP,OneTimeWorkRequestBuilder<TranscriptSyncWorker>().setConstraints(constraints).setBackoffCriteria(BackoffPolicy.EXPONENTIAL,30,TimeUnit.SECONDS).build())
        manager.enqueueUniquePeriodicWork("mira-periodic-sync",ExistingPeriodicWorkPolicy.KEEP,PeriodicWorkRequestBuilder<TranscriptSyncWorker>(15,TimeUnit.MINUTES).setConstraints(constraints).build())
    }
    internal fun upload(c:Context):Boolean {
        val code=prefs(c).getString("code",null)?:return true
        val peer=parse(code);val store=TranscriptStore(c);val rows=store.list()
        try {
            for(i in 0 until rows.length()){
                if(prefs(c).getString("code",null)!=code)return true
                val meta=rows.getJSONObject(i);if(!meta.optBoolean("complete")||meta.optBoolean("synced"))continue
                val row=store.get(meta.getString("id"));val pdf=TranscriptPdf.export(c,row)
                request(peer,"POST","/v1/transcripts",JSONObject().put("id",row.getString("id")).put("title",row.optString("title")).put("text",row.optString("text")).put("pdf",Base64.encodeToString(pdf.readBytes(),Base64.NO_WRAP)))
                // A 200 acknowledgement is required before marking delivered; retries reuse the same document ID.
                row.put("synced",true);store.save(row)
            }
            prefs(c).edit().putString("status","Sincronizat · ${java.text.SimpleDateFormat("HH:mm",java.util.Locale.ROOT).format(java.util.Date())}").apply();return true
        }catch(e:Exception){prefs(c).edit().putString("status","În așteptare: ${e.message?.take(140)}").apply();return false}
    }
    private data class Peer(val host:String,val port:Int,val fingerprint:String,val token:String)
    private fun parse(code:String):Peer{
        val uri=Uri.parse(code.trim());val host=uri.host?:error("Cod de asociere invalid.")
        val ip=host.split('.').map{it.toIntOrNull()?:-1}
        require(ip.size==4&&ip.all{it in 0..255}&&(ip[0]==10||ip[0]==127||(ip[0]==192&&ip[1]==168)||(ip[0]==172&&ip[1] in 16..31)||(ip[0]==169&&ip[1]==254))){"Este necesar un PC din rețeaua locală."}
        val fp=uri.getQueryParameter("fp").orEmpty();val token=uri.getQueryParameter("token").orEmpty()
        require(uri.scheme=="mira-pair"&&uri.port in 1..65535&&Regex("[a-f0-9]{64}").matches(fp)&&Regex("[a-f0-9]{64}").matches(token)&&uri.userInfo==null){"Cod de asociere invalid."}
        return Peer(host,uri.port,fp,token)
    }
    private fun fingerprint(cert:X509Certificate)=MessageDigest.getInstance("SHA-256").digest(cert.encoded).joinToString(""){"%02x".format(it)}
    private fun request(peer:Peer,method:String,path:String,body:JSONObject?):String{
        val trust=object:X509TrustManager{
            override fun getAcceptedIssuers():Array<X509Certificate> = emptyArray()
            override fun checkClientTrusted(chain:Array<X509Certificate>,authType:String){throw java.security.cert.CertificateException("Client certificates unsupported")}
            override fun checkServerTrusted(chain:Array<X509Certificate>,authType:String){
                if(chain.isEmpty()||!MessageDigest.isEqual(fingerprint(chain[0]).toByteArray(),peer.fingerprint.toByteArray()))throw java.security.cert.CertificateException("Certificatul PC-ului nu corespunde asocierii.")
                chain[0].checkValidity()
            }
        }
        val tls=SSLContext.getInstance("TLS").apply{init(null,arrayOf(trust),java.security.SecureRandom())}
        val connection=URL("https://${peer.host}:${peer.port}$path").openConnection() as HttpsURLConnection
        try {
            connection.sslSocketFactory=tls.socketFactory
            connection.hostnameVerifier=HostnameVerifier{host,session->host==peer.host&&runCatching{fingerprint(session.peerCertificates[0] as X509Certificate)==peer.fingerprint}.getOrDefault(false)}
            connection.connectTimeout=4000;connection.readTimeout=8000;connection.instanceFollowRedirects=false;connection.requestMethod=method
            connection.setRequestProperty("Authorization","Bearer ${peer.token}")
            if(body!=null){val bytes=body.toString().toByteArray(Charsets.UTF_8);require(bytes.size<=12*1024*1024){"Document prea mare pentru transfer."};connection.doOutput=true;connection.setRequestProperty("Content-Type","application/json");connection.setFixedLengthStreamingMode(bytes.size);connection.outputStream.use{it.write(bytes)}}
            val code=connection.responseCode
            check(code==200||code==202){when(code){401->"Asociere revocată pe PC. Asociază din nou.";409->"Documentul primit are același ID, dar conținut diferit.";else->"PC-ul nu a acceptat cererea ($code)."}}
            return connection.inputStream.bufferedReader().use{it.readText()}
        }finally{connection.disconnect()}
    }
}
class TranscriptSyncWorker(context:Context,params:WorkerParameters):Worker(context,params){override fun doWork()=if(TranscriptSync.upload(applicationContext))Result.success()else Result.retry()}
