package io.hoenn.sessions;

import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioTrack;

final class AudioSink implements AutoCloseable {
    private final AudioTrack track;

    AudioSink() {
        int size = Math.max(8192, AudioTrack.getMinBufferSize(32768,
                AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT));
        track = new AudioTrack.Builder()
                .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
                .setAudioFormat(new AudioFormat.Builder().setSampleRate(32768)
                        .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                .setBufferSizeInBytes(size).setTransferMode(AudioTrack.MODE_STREAM)
                .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY).build();
        if (track.getState() != AudioTrack.STATE_INITIALIZED) {
            track.release();
            throw new IllegalStateException("Audio unavailable");
        }
        track.play();
    }

    void pause() {
        if (track.getPlayState() == AudioTrack.PLAYSTATE_PLAYING) track.pause();
    }

    void resume() {
        if (track.getPlayState() != AudioTrack.PLAYSTATE_PLAYING) track.play();
    }

    boolean write(short[] samples, int count, boolean audible) {
        track.setVolume(audible ? 1f : 0f);
        if (!audible) return false;
        for (int at = 0; at < count;) {
            int written = track.write(samples, at, count - at, AudioTrack.WRITE_BLOCKING);
            if (written <= 0) return false;
            at += written;
        }
        return count > 0;
    }

    @Override public void close() {
        try { track.stop(); } finally { track.release(); }
    }
}
