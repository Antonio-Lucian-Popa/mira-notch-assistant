# Local Romanian voice

Mira uses whisper.cpp v1.8.3 with the multilingual Whisper **base** model. There
is no paid API, account, key or per-command fee. The native library and model
weights are MIT licensed. Attribution files are packaged in `assets/licenses/`.

## First use

Open **Voce & PC**, download the free model (147,951,465 bytes, about 148 MB), then
enable **Activare vocală Mira** and allow microphone access. Downloading requires
internet; recognition afterwards is local. Stop from the switch or the ongoing
microphone notification. The opted-in service can resume when the app is opened,
but does not start at boot. Android can stop it in the background.

Say `Mira, pornește focus`, `Mira, am băut apă`, or `Mira, pornește transcrierea`.
You can also say `Mira`, pause, then give a command within 15 seconds. `Mira gata`
saves dictation and returns to wake listening when automatic activation is enabled.
Manual transcription stops the microphone when finished. Dictation has a
30-minute limit. Only the fixed command whitelist can execute actions.

This is speech-gated ASR, not a low-power hardware hotword detector. It processes
bounded utterances after a short silence, so recognition has latency and accuracy
varies with pronunciation, noise and device speed. Leave a brief pause between
phrases. It uses CPU, memory and battery while enabled; unrelated nearby speech
can also trigger local computation. A bounded queue prevents unlimited memory
use; overload is reported rather than queued indefinitely. Text should be checked.

## Privacy and integrity

Raw PCM stays in short in-memory buffers and is not written to files or uploaded.
Unrelated recognized speech is discarded unless dictation is active. Requested
transcripts are saved locally and retain the existing explicitly paired-PC sync
behavior. The only voice-model network request downloads weights over HTTPS from
`huggingface.co/ggerganov/whisper.cpp`. No recorded audio is sent there.

The download is bounded to the expected size, verified against SHA-256
`60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe`, then atomically
moved into private app storage. Partial or failed downloads cannot become models.

## Native build

Gradle downloads the pinned whisper.cpp source archive from GitHub and verifies
SHA-256 `870ba21409cdf66697dc4db15ebdb13bc67037d76c7cc63756c81471d8f1731a`.
Sources stay under the ignored Android build directory. The initial build needs
network access; subsequent builds can reuse the cached verified sources. CMake
links a CPU-only JNI library for arm64-v8a and x86_64. Model binaries are not
committed to Git or bundled into the APK.

Run `npm test -- --runInBand`, `npm run typecheck`, `npm run lint`, and Android
`:app:testPreviewDebugUnitTest`. The optional `LocalWhisperTest` instrumented test
requires a downloaded model and an installed offline Romanian TTS voice; it uses
synthesized test phrases, never the microphone or user reminders. It checks native
Romanian decoding, wake/command matching, timing and silence rejection. Actual
microphone accuracy and battery use must also be tested on real hardware.

Sources: https://github.com/ggml-org/whisper.cpp/tree/v1.8.3 and
https://github.com/openai/whisper (code and model weights under MIT).
