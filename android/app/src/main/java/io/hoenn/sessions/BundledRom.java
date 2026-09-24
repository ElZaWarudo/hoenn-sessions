package io.hoenn.sessions;

import java.io.*;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.Properties;

/** Installs only the ROM identified by this APK, without touching save files. */
final class BundledRom {
    interface Source { InputStream open() throws IOException; }
    private static final long MAX_BYTES = 64L * 1024 * 1024;

    static void install(File directory, String expectedHash, Source source) throws Exception {
        install(directory, expectedHash, source, null);
    }

    static void install(File directory, String expectedHash, Source source, VerifiedHashCache cache) throws Exception {
        File rom = new File(directory, "pokeemerald.gba");
        if (rom.isFile() && (cache == null ? expectedHash.equals(hash(rom)) : cache.verify(rom, rom.length(), expectedHash))) return;
        File temporary = new File(directory, "bundled-rom.tmp");
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            try (InputStream input = source.open(); FileOutputStream output = new FileOutputStream(temporary)) {
                byte[] buffer = new byte[65536];
                long total = 0;
                int count;
                while ((count = input.read(buffer)) != -1) {
                    total += count;
                    if (total > MAX_BYTES) throw new IOException("ROM demasiado grande");
                    digest.update(buffer, 0, count);
                    output.write(buffer, 0, count);
                }
                if (!expectedHash.equals(ReleaseCatalog.hex(digest.digest()))) throw new IOException("La ROM incluida no coincide con esta versión");
                output.getFD().sync();
            }
            Files.move(temporary.toPath(), rom.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            if (cache != null) cache.remember(rom, expectedHash);
        } finally {
            Files.deleteIfExists(temporary.toPath());
        }
    }

    static String hash(File file) throws Exception {
        if (file.length() > MAX_BYTES) return "";
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream input = new FileInputStream(file)) {
            byte[] buffer = new byte[65536];
            int count;
            while ((count = input.read(buffer)) != -1) digest.update(buffer, 0, count);
        }
        return ReleaseCatalog.hex(digest.digest());
    }

    /** App-private hint; a miss always verifies file bytes before recording metadata. */
    static final class VerifiedHashCache {
        private final File marker;
        VerifiedHashCache(File marker) { this.marker = marker; }

        boolean verify(File file, long expectedSize, String expectedHash) throws Exception {
            if (Files.isSymbolicLink(file.toPath()) || !file.isFile() || file.length() != expectedSize
                || expectedSize <= 0 || expectedSize > MAX_BYTES) return false;
            String key = file.getCanonicalPath();
            String stamp = expectedSize + ":" + file.lastModified() + ":" + expectedHash;
            Properties entries = read();
            if (stamp.equals(entries.getProperty(key))) return true;
            if (!expectedHash.equals(hash(file))) return false;
            entries.setProperty(key, stamp);
            write(entries);
            return true;
        }

        void remember(File file, String expectedHash) {
            if (Files.isSymbolicLink(file.toPath()) || !file.isFile()) return;
            Properties entries = read();
            try {
                entries.setProperty(file.getCanonicalPath(), file.length() + ":" + file.lastModified() + ":" + expectedHash);
                write(entries);
            } catch (IOException ignored) { /* Cache failures never prevent use of a verified ROM. */ }
        }

        private Properties read() {
            Properties entries = new Properties();
            if (Files.isSymbolicLink(marker.toPath()) || !marker.isFile() || marker.length() > 65536) return entries;
            try (InputStream input = new FileInputStream(marker)) { entries.load(input); }
            catch (IOException ignored) { entries.clear(); }
            return entries;
        }

        private void write(Properties entries) {
            File temporary = new File(marker.getParentFile(), marker.getName() + ".tmp");
            try {
                try (FileOutputStream output = new FileOutputStream(temporary)) {
                    entries.store(output, "Verified ROM hashes");
                    output.getFD().sync();
                }
                Files.move(temporary.toPath(), marker.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            } catch (IOException ignored) { /* A cache write is optional. */ }
            finally { try { Files.deleteIfExists(temporary.toPath()); } catch (IOException ignored) { } }
        }
    }
}
