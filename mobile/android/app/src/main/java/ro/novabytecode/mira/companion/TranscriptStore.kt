package ro.novabytecode.mira.companion

import android.content.Context
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

class TranscriptStore(context: Context) {
    private val folder = File(context.filesDir,"transcripts").apply {mkdirs()}
    companion object {private val lock = Any()}
    fun list(): JSONArray = synchronized(lock) {
        val catalog=File(folder,"catalog")
        if(catalog.exists()) return@synchronized runCatching{JSONArray(catalog.readText())}.getOrDefault(JSONArray())
        val rows=JSONArray();folder.listFiles()?.filter{it.extension=="json"}?.sortedByDescending{it.lastModified()}?.forEach{file->runCatching{rows.put(metadata(JSONObject(file.readText())))}}
        writeCatalog(rows);rows
    }
    private fun metadata(row:JSONObject)=JSONObject(row.toString()).apply{val text=optString("text");remove("text");put("preview",text.take(160));put("characters",text.length)}
    private fun writeCatalog(rows:JSONArray){val file=AtomicFile(File(folder,"catalog"));val out=file.startWrite();try{out.write(rows.toString().toByteArray(Charsets.UTF_8));file.finishWrite(out)}catch(e:Exception){file.failWrite(out);throw e}}
    fun create(): JSONObject = synchronized(lock) {
        require(list().length()<100) {"Ai 100 de transcrieri. Șterge câteva înainte de a începe."}
        JSONObject().put("id",UUID.randomUUID().toString()).put("createdAt",System.currentTimeMillis())
            .put("title","Transcriere Mira").put("text","").put("complete",false).put("synced",false).also {save(it)}
    }
    fun get(id: String): JSONObject = synchronized(lock) {validate(id);JSONObject(File(folder,"$id.json").readText())}
    fun save(row: JSONObject) = synchronized(lock) {
        val id = row.getString("id");validate(id)
        val atomic=AtomicFile(File(folder,"$id.json"));val out=atomic.startWrite()
        try {out.write(row.toString().toByteArray(Charsets.UTF_8));atomic.finishWrite(out)
            val previous=list();val index=JSONArray().put(metadata(row));for(i in 0 until previous.length()){val item=previous.getJSONObject(i);if(item.getString("id")!=id)index.put(item)};writeCatalog(index)
        } catch(e:Exception){atomic.failWrite(out);throw e}
    }
    fun delete(id: String) = synchronized(lock) {validate(id);File(folder,"$id.json").delete();File(folder,"$id.pdf").delete();val all=list();val kept=JSONArray();for(i in 0 until all.length()){if(all.getJSONObject(i).getString("id")!=id)kept.put(all.getJSONObject(i))};writeCatalog(kept);Unit}
    fun pdf(id: String) = File(folder,"${validate(id)}.pdf")
    private fun validate(id:String):String {require(Regex("[a-f0-9-]{36}").matches(id)){"ID invalid"};return id}
}
