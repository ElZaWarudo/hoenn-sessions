#include <jni.h>
#include <android/bitmap.h>
#include <mgba/core/core.h>
#include <mgba/gba/core.h>
#include <mgba/core/version.h>
#include <mgba-util/vfs.h>
#include <mgba/core/blip_buf.h>
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <stdio.h>

// All access is serialized by NativeCore's Java monitor, including frame boundaries.
static struct mCore* core;
static color_t pixels[240 * 160];
static uint32_t bridge_address, generation_address, callback_generation;
static uint64_t callback_serial;
static char* save_path;
static void savedata_updated(void* context) {
    (void)context;
    ++callback_serial;
    if(core && generation_address) callback_generation=core->busRead32(core,generation_address);
}
static struct mCoreCallbacks callbacks={.savedataUpdated=savedata_updated};
JNIEXPORT jstring JNICALL Java_io_hoenn_sessions_NativeCore_identity(JNIEnv* env, jclass cls) {
    (void)cls;
    char identity[160];
    snprintf(identity,sizeof(identity),"%s|%s",projectVersion,gitCommit);
    return (*env)->NewStringUTF(env,identity);
}
static void close_core(void) {
    if (core) { mCoreConfigDeinit(&core->config); core->deinit(core); core = NULL; }
    free(save_path);save_path=NULL;callback_serial=0;callback_generation=0;
}
JNIEXPORT jboolean JNICALL Java_io_hoenn_sessions_NativeCore_open(JNIEnv* env, jclass cls, jstring rom, jstring save) {
    (void) cls;
    close_core();
    const char* path = (*env)->GetStringUTFChars(env, rom, NULL);
    core = GBACoreCreate();
    if (!core || !core->init(core)) { core = NULL; (*env)->ReleaseStringUTFChars(env, rom, path); return JNI_FALSE; }
    mCoreInitConfig(core, NULL);
    core->setVideoBuffer(core, pixels, 240);
    core->setAudioBufferSize(core, 2048);
    blip_set_rates(core->getAudioChannel(core, 0), core->frequency(core), 32768);
    blip_set_rates(core->getAudioChannel(core, 1), core->frequency(core), 32768);
    bool ok = mCoreLoadFile(core, path);
    (*env)->ReleaseStringUTFChars(env, rom, path);
    if (!ok) { close_core(); return JNI_FALSE; }
    path = (*env)->GetStringUTFChars(env, save, NULL);
    save_path=strdup(path);
    struct VFile* vf = VFileOpen(path, O_CREAT | O_RDWR);
    ok = vf && core->loadSave(core, vf);
    (*env)->ReleaseStringUTFChars(env, save, path);
    if (!ok) { if (vf) vf->close(vf); close_core(); return JNI_FALSE; }
    core->addCoreCallbacks(core,&callbacks);
    core->reset(core);
    return JNI_TRUE;
}
JNIEXPORT void JNICALL Java_io_hoenn_sessions_NativeCore_close(JNIEnv* env, jclass cls) { (void) env; (void) cls; close_core(); }
JNIEXPORT jint JNICALL Java_io_hoenn_sessions_NativeCore_frame(JNIEnv* env, jclass cls, jint keys, jobject bitmap, jshortArray audio) {
    (void) cls;
    AndroidBitmapInfo info;
    if (!core || !bitmap || AndroidBitmap_getInfo(env,bitmap,&info)!=ANDROID_BITMAP_RESULT_SUCCESS
        || info.width!=240 || info.height!=160 || info.format!=ANDROID_BITMAP_FORMAT_RGBA_8888
        || info.stride<240*4 || (*env)->GetArrayLength(env, audio) < 4096) return -1;
    core->setKeys(core, (uint32_t) keys & 1023);
    core->runFrame(core);
    void* bitmap_pixels=NULL;
    if(AndroidBitmap_lockPixels(env,bitmap,&bitmap_pixels)!=ANDROID_BITMAP_RESULT_SUCCESS)return -1;
    for(int y=0;y<160;++y){
        uint32_t* row=(uint32_t*)((uint8_t*)bitmap_pixels+y*info.stride);
        for(int x=0;x<240;++x)row[x]=0xff000000u|((uint32_t)pixels[y*240+x]&0x00ffffffu);
    }
    AndroidBitmap_unlockPixels(env,bitmap);
    short samples[4096];
    int n=blip_read_samples(core->getAudioChannel(core, 0), samples, 2048, true);
    int r=blip_read_samples(core->getAudioChannel(core, 1), samples+1, 2048, true);
    if (r<n) n=r;
    (*env)->SetShortArrayRegion(env, audio, 0, n*2, samples);
    return n*2;
}
#define BRIDGE_ADDRESS bridge_address
static bool valid_bridge(void) {
    return core && bridge_address && core->busRead32(core,BRIDGE_ADDRESS)==1347109711u
        && core->busRead16(core,BRIDGE_ADDRESS+4)==1 && core->busRead16(core,BRIDGE_ADDRESS+6)==1
        && core->busRead32(core,BRIDGE_ADDRESS+8)==65536;
}

static jbyteArray read_bridge_bytes(JNIEnv* env,uint32_t address,jsize length) {
    jbyte bytes[144];
    for(jsize i=0;i<length;++i)bytes[i]=(jbyte)core->busRead8(core,address+(uint32_t)i);
    jbyteArray result=(*env)->NewByteArray(env,length);
    if(result)(*env)->SetByteArrayRegion(env,result,0,length,bytes);
    return result;
}
JNIEXPORT jbyteArray JNICALL Java_io_hoenn_sessions_NativeCore_bridgeHeader(JNIEnv* env,jclass cls) {
    (void)cls;
    if(!valid_bridge())return NULL;
    return read_bridge_bytes(env,BRIDGE_ADDRESS,24);
}
JNIEXPORT jbyteArray JNICALL Java_io_hoenn_sessions_NativeCore_bridgeSlot(JNIEnv* env,jclass cls,jint index) {
    (void)cls;
    if(!valid_bridge() || index<0 || index>=32)return NULL;
    return read_bridge_bytes(env,BRIDGE_ADDRESS+24+(uint32_t)index*144,144);
}

JNIEXPORT void JNICALL Java_io_hoenn_sessions_NativeCore_configureBridge(JNIEnv* env,jclass cls,jint address,jint generation) {
    (void)env;(void)cls;
    if(core)return;
    bridge_address=(address>=0x02000000 && address<=0x02040000-9244 && !(address&3))?(uint32_t)address:0;
    generation_address=(generation>=0x02000000 && generation<=0x02040000-4 && !(generation&3))?(uint32_t)generation:0;
}
JNIEXPORT jlongArray JNICALL Java_io_hoenn_sessions_NativeCore_saveEvidence(JNIEnv* env,jclass cls) {
    (void)cls;
    jlong values[3]={(jlong)callback_serial,(jlong)callback_generation,core&&generation_address?(jlong)core->busRead32(core,generation_address):-1};
    jlongArray result=(*env)->NewLongArray(env,3);if(result)(*env)->SetLongArrayRegion(env,result,0,3,values);return result;
}
JNIEXPORT jboolean JNICALL Java_io_hoenn_sessions_NativeCore_syncSave(JNIEnv* env,jclass cls) {
    (void)env;(void)cls;
    if(!core || !save_path || !callback_serial)return JNI_FALSE;
    void* bytes=NULL;size_t size=core->savedataClone(core,&bytes);
    if(size!=128*1024 || !bytes){free(bytes);return JNI_FALSE;}
    // Persist only actual emulator-produced Flash1M bytes, after the matching
    // savedata callback; the Rust parser still verifies the complete container.
    int fd=open(save_path,O_WRONLY);bool ok=fd>=0;
    for(size_t at=0;ok && at<size;){ssize_t n=write(fd,(char*)bytes+at,size-at);if(n<=0)ok=false;else at+=(size_t)n;}
    if(ok)ok=ftruncate(fd,(off_t)size)==0 && fsync(fd)==0;
    if(fd>=0)close(fd);free(bytes);return ok?JNI_TRUE:JNI_FALSE;
}
JNIEXPORT void JNICALL Java_io_hoenn_sessions_NativeCore_bridgeHeartbeat(JNIEnv* env,jclass cls) {
    (void)env;(void)cls;if(valid_bridge())core->busWrite32(core,BRIDGE_ADDRESS+16,core->busRead32(core,BRIDGE_ADDRESS+16)+1);
}
JNIEXPORT jboolean JNICALL Java_io_hoenn_sessions_NativeCore_bridgeCounter(JNIEnv* env,jclass cls,jboolean inbound,jint expected,jint value) {
    (void)env;(void)cls;
    if(!valid_bridge() || expected<0 || expected>65535 || value<0 || value>65535)return JNI_FALSE;
    uint32_t address=BRIDGE_ADDRESS+(inbound?4634:20);
    if(core->busRead16(core,address)!=(uint32_t)expected)return JNI_FALSE;
    core->busWrite16(core,address,(uint16_t)value);return JNI_TRUE;
}
JNIEXPORT jboolean JNICALL Java_io_hoenn_sessions_NativeCore_bridgePush(JNIEnv* env,jclass cls,jbyteArray frame) {
    (void)cls;
    if(!valid_bridge() || (*env)->GetArrayLength(env,frame)!=144)return JNI_FALSE;
    uint32_t queue=BRIDGE_ADDRESS+4632;
    uint16_t read=(uint16_t)core->busRead16(core,queue),write=(uint16_t)core->busRead16(core,queue+2);
    uint16_t depth=(uint16_t)(write-read);
    if(depth>32){core->busWrite16(core,queue+2,read);return JNI_FALSE;}
    if(depth==32)return JNI_FALSE;
    jbyte bytes[144];(*env)->GetByteArrayRegion(env,frame,0,144,bytes);
    uint32_t slot=queue+4+(write&31)*144;
    for(int i=0;i<144;++i)core->busWrite8(core,slot+i,(uint8_t)bytes[i]);
    // Publish last, with the emulated CPU stopped under the NativeCore monitor.
    core->busWrite16(core,queue+2,(uint16_t)(write+1));return JNI_TRUE;
}
