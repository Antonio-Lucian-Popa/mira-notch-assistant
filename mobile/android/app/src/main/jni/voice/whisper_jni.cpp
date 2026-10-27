#include <jni.h>
#include <whisper.h>
#include <atomic>
#include <algorithm>
#include <thread>
#include <string>
#include <vector>

struct Engine {
    whisper_context * ctx;
    std::atomic<bool> cancelled{false};
};
static bool aborted(void * user) { return static_cast<Engine *>(user)->cancelled.load(); }
static void failure(JNIEnv * env, const char * message) {
    env->ThrowNew(env->FindClass("java/lang/IllegalStateException"), message);
}
extern "C" JNIEXPORT jlong JNICALL
Java_ro_novabytecode_mira_companion_LocalWhisper_load(JNIEnv * env, jobject, jstring path) {
    const char * chars = env->GetStringUTFChars(path, nullptr);
    auto config = whisper_context_default_params();
    config.use_gpu = false;
    auto * ctx = whisper_init_from_file_with_params(chars, config);
    env->ReleaseStringUTFChars(path, chars);
    if (!ctx) { failure(env, "Nu pot încărca modelul vocal local."); return 0; }
    return reinterpret_cast<jlong>(new Engine{ctx});
}
extern "C" JNIEXPORT jstring JNICALL
Java_ro_novabytecode_mira_companion_LocalWhisper_transcribe(JNIEnv * env, jobject, jlong handle, jfloatArray audio, jboolean command_mode) {
    auto * engine = reinterpret_cast<Engine *>(handle);
    if (!engine || engine->cancelled.load()) return env->NewStringUTF("");
    const auto count = env->GetArrayLength(audio);
    if (count < 3200 || count > 16000 * 15) { failure(env, "Segment audio invalid."); return nullptr; }
    std::vector<float> samples(count);
    env->GetFloatArrayRegion(audio, 0, count, samples.data());
    auto params = whisper_full_default_params(WHISPER_SAMPLING_BEAM_SEARCH);
    params.n_threads = std::min(4, std::max(1, static_cast<int>(std::thread::hardware_concurrency())));
    params.language = "ro";
    params.translate = false;
    params.no_context = true;
    params.no_timestamps = true;
    params.single_segment = true;
    params.print_progress = false;
    params.print_realtime = false;
    params.print_timestamps = false;
    params.print_special = false;
    params.suppress_nst = true;
    params.beam_search.beam_size = 5;
    params.initial_prompt = command_mode
        ? "Asistentul Mira. Comenzi în română: pornește focus, pune pauză, continuă focus, oprește focus, am băut apă, adaugă un pahar de apă, pornește transcrierea, Mira gata."
        : "Conversație în limba română. Asistentul se numește Mira. Mira gata.";
    params.temperature = 0.0f;
    params.temperature_inc = 0.0f;
    params.abort_callback = aborted;
    params.abort_callback_user_data = engine;
    const int status = whisper_full(engine->ctx, params, samples.data(), count);
    if (engine->cancelled.load()) return env->NewStringUTF("");
    if (status != 0) { failure(env, "Transcrierea locală a eșuat."); return nullptr; }
    std::string text;
    for (int i = 0; i < whisper_full_n_segments(engine->ctx); ++i) {
        if (whisper_full_get_segment_no_speech_prob(engine->ctx, i) < 0.65f)
            text += whisper_full_get_segment_text(engine->ctx, i);
    }
    return env->NewStringUTF(text.c_str());
}
extern "C" JNIEXPORT void JNICALL
Java_ro_novabytecode_mira_companion_LocalWhisper_cancel(JNIEnv *, jobject, jlong handle) {
    if (handle) reinterpret_cast<Engine *>(handle)->cancelled.store(true);
}
extern "C" JNIEXPORT void JNICALL
Java_ro_novabytecode_mira_companion_LocalWhisper_release(JNIEnv *, jobject, jlong handle) {
    auto * engine = reinterpret_cast<Engine *>(handle);
    if (engine) { whisper_free(engine->ctx); delete engine; }
}
