package ro.novabytecode.mira.companion
import org.junit.Assert.*
import org.junit.Test
class VoiceRulesTest {
    @Test fun romanianAsrAccentOnTheNameStillStopsWithoutSavingTheCommand(){
        val b=DictationBuffer();assertTrue(b.accept("Ședința s-a încheiat. Miră, gata!"))
        assertEquals("Ședința s-a încheiat.",b.finish())
    }
    @Test fun commandsRequireTheWakeBoundaryAndStayWhitelisted(){
        assertEquals("porneste transcrierea",VoiceRules.wake("Hey Mira, pornește transcrierea!"))
        assertEquals("dictate",VoiceRules.command("Pornește transcrierea"))
        assertNull(VoiceRules.wake("Admirabil"));assertNull(VoiceRules.wake("miracol pornește"));assertNull(VoiceRules.command("șterge toate fișierele"))
    }
    @Test fun stopPhraseIsRemovedEvenAcrossRecognitionChunks(){
        val b=DictationBuffer();assertFalse(b.accept("Astăzi discutăm despre proiect. Hey Mira"));assertTrue(b.accept("gata și alte cuvinte"))
        val text=b.finish();assertTrue(text.contains("proiect."));assertFalse(text.contains("Mira"));assertFalse(text.contains("gata"));assertFalse(text.contains("alte"))
    }
    @Test fun manualStopKeepsOrdinaryWordsAndRomanianDiacritics(){val b=DictationBuffer();b.accept("Ședință mâine. Mira este numele proiectului.");assertEquals("Ședință mâine. Mira este numele proiectului.",b.finish().replace('\n',' '))}
    @Test fun punctuationAndSingleUtteranceStopAreHandled(){val b=DictationBuffer();assertTrue(b.accept("Am terminat. Mira, gata!"));assertEquals("Am terminat.",b.finish())}
}
