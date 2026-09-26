package io.hoenn.sessions;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Rect;
import android.os.SystemClock;
import android.util.Log;
import android.view.View;
import java.util.concurrent.TimeUnit;

final class GameRenderer extends View implements Runnable {
    interface Host {
        boolean isResumed();
        boolean hasInputFocus();
        boolean hasAudioFocus();
        boolean isStarting();
        boolean isSettingsOpen();
        boolean isIntegerScale();
        boolean showStats();
        BridgeConnection connection();
        void onStats(int fps);
        void onRendererError(Exception error);
    }

    volatile int keys;
    private final Host host;
    private final TouchOverlay touch;
    private final ControllerInput controller;
    private final Bitmap bitmap = Bitmap.createBitmap(240, 160, Bitmap.Config.ARGB_8888);
    private final Paint paint = new Paint();
    private Thread thread;
    private volatile boolean stop;

    GameRenderer(Context context, Host host, TouchOverlay touch, ControllerInput controller, boolean smooth) {
        super(context);
        this.host = host;
        this.touch = touch;
        this.controller = controller;
        paint.setFilterBitmap(smooth);
    }

    void setSmoothPixels(boolean value) { paint.setFilterBitmap(value); invalidate(); }
    void start() { if (thread != null && thread.isAlive()) return; stop = false; thread = new Thread(this, "gba-frame"); thread.start(); }
    void stop() { stop = true; if (thread != null) { try { thread.join(1000); } catch (InterruptedException e) { Thread.currentThread().interrupt(); } } }

    @Override public void run() {
        AudioSink audio = null;
        try { audio = new AudioSink(); }
        catch (IllegalArgumentException | IllegalStateException error) { Log.w("HoennGame", "Audio unavailable", error); }
        short[] samples = new short[4096];
        int frameCount = 0;
        long fpsAt = SystemClock.elapsedRealtime();
        try {
            while (!stop && (host.isResumed() || host.isStarting() || NativeSession.isActive())) {
                if (!host.isResumed() || !host.hasInputFocus() || !host.hasAudioFocus()) {
                    if (audio != null) audio.pause();
                    synchronized (NativeCore.class) { BridgeConnection connection = host.connection(); if (connection != null) connection.step(); }
                    TimeUnit.MILLISECONDS.sleep(100);
                    continue;
                }
                if (audio != null) audio.resume();
                long started = System.nanoTime();
                boolean fast = touch.fastForwardHeld() || controller.fastForwardHeld();
                int repeats = fast ? 4 : 1, count = -1;
                for (int i = 0; i < repeats; i++) {
                    synchronized (NativeCore.class) {
                        int input = host.isResumed() && host.hasInputFocus() && !host.isSettingsOpen()
                                ? keys | touch.keys() | controller.keys() : 0;
                        synchronized (bitmap) { count = NativeCore.frame(input, bitmap, samples); }
                        BridgeConnection connection = host.connection();
                        if (connection != null) connection.step();
                    }
                    if (count < 0) break;
                }
                if (count < 0) {
                    TimeUnit.MILLISECONDS.sleep(16);
                    continue;
                }
                boolean pacedByAudio = false;
                if (count >= 0) {
                    postInvalidate();
                    if (audio != null) pacedByAudio = audio.write(samples, count, host.isResumed() && host.hasAudioFocus() && !fast);
                    frameCount += repeats;
                }
                long now = SystemClock.elapsedRealtime();
                if (host.showStats() && now - fpsAt >= 1000) {
                    int fps = Math.round(frameCount * 1000f / (now - fpsAt));
                    frameCount = 0; fpsAt = now;
                    host.onStats(fps);
                } else if (!host.showStats()) { frameCount = 0; fpsAt = now; }
                if (!pacedByAudio) {
                    long left = 16742706 - (System.nanoTime() - started);
                    if (left > 0) TimeUnit.NANOSECONDS.sleep(left);
                }
            }
        } catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        catch (Exception error) { host.onRendererError(error); }
        finally { if (audio != null) audio.close(); }
    }

    @Override protected void onMeasure(int widthSpec, int heightSpec) {
        int width = MeasureSpec.getSize(widthSpec);
        setMeasuredDimension(width, resolveSize(width * 2 / 3, heightSpec));
    }

    @Override protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        canvas.drawColor(Color.BLACK);
        float scale = Math.min(getWidth() / 240f, getHeight() / 160f);
        if (host.isIntegerScale() && scale >= 1f) scale = (float) Math.floor(scale);
        int width = Math.round(240 * scale), height = Math.round(160 * scale);
        Rect destination = new Rect((getWidth() - width) / 2, (getHeight() - height) / 2,
                (getWidth() + width) / 2, (getHeight() + height) / 2);
        synchronized (bitmap) { canvas.drawBitmap(bitmap, null, destination, paint); }
    }
}
