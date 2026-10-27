package ro.novabytecode.mira.companion

import android.content.Context
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.pdf.PdfDocument
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import org.json.JSONObject
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

object TranscriptPdf {
    @Synchronized fun export(context:Context,row:JSONObject):File {
        val target=TranscriptStore(context).pdf(row.getString("id"))
        require(row.optBoolean("complete")){"Oprește mai întâi transcrierea."}
        if(target.exists())return target
        val text=row.optString("text").ifBlank{"(Nu a fost recunoscut niciun text.)"}
        val body=TextPaint(Paint.ANTI_ALIAS_FLAG).apply{color=Color.rgb(36,49,62);textSize=12f;typeface=Typeface.create("sans-serif",Typeface.NORMAL)}
        val layout=StaticLayout.Builder.obtain(text,0,text.length,body,491).setAlignment(Layout.Alignment.ALIGN_NORMAL)
            .setLineSpacing(4f,1f).setIncludePad(false).build()
        val ranges=mutableListOf<Pair<Int,Int>>();var start=0
        while(start<layout.lineCount){var end=start+1;while(end<layout.lineCount&&layout.getLineBottom(end)-layout.getLineTop(start)<=625)end++;ranges.add(start to end);start=end}
        val document=PdfDocument()
        val date=SimpleDateFormat("dd MMMM yyyy · HH:mm",Locale.forLanguageTag("ro-RO")).format(Date(row.optLong("createdAt")))
        try {
            ranges.forEachIndexed{index,(first,last)->
                val page=document.startPage(PdfDocument.PageInfo.Builder(595,842,index+1).create());val c=page.canvas
                val paint=Paint(Paint.ANTI_ALIAS_FLAG)
                c.drawColor(Color.WHITE)
                paint.color=Color.rgb(16,33,43);c.drawRect(0f,0f,595f,116f,paint)
                paint.color=Color.rgb(133,239,183);c.drawRoundRect(52f,30f,60f,43f,4f,4f,paint);c.drawRoundRect(70f,30f,78f,43f,4f,4f,paint)
                paint.textSize=11f;paint.typeface=Typeface.create("sans-serif-medium",Typeface.NORMAL);c.drawText("MIRA  /  VOCE",93f,41f,paint)
                paint.color=Color.WHITE;paint.textSize=21f
                val title=android.text.TextUtils.ellipsize(row.optString("title","Transcriere Mira"),TextPaint(paint),491f,android.text.TextUtils.TruncateAt.END).toString()
                c.drawText(title,52f,78f,paint)
                paint.color=Color.rgb(182,204,213);paint.textSize=10f;c.drawText(date,52f,99f,paint)
                for(line in first until last){
                    val from=layout.getLineStart(line);val to=layout.getLineEnd(line)
                    c.drawText(text,from,to,52f,142f+layout.getLineBaseline(line)-layout.getLineTop(first),body)
                }
                paint.color=Color.rgb(214,225,228);paint.strokeWidth=1f;c.drawLine(52f,796f,543f,796f,paint)
                paint.color=Color.rgb(106,122,134);paint.textSize=9f;c.drawText("Transcriere automată · textul poate necesita corecturi",52f,816f,paint)
                paint.textAlign=Paint.Align.RIGHT;c.drawText("${index+1} / ${ranges.size}",543f,816f,paint)
                document.finishPage(page)
            }
            val tmp=File(target.parentFile,target.name+".part")
            tmp.outputStream().use{document.writeTo(it)}
            check(tmp.renameTo(target)){"Nu pot salva PDF-ul."}
            return target
        }finally{document.close()}
    }
}
