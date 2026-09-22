package io.hoenn.sessions;

import java.io.*;
import java.nio.file.*;
import java.security.MessageDigest;

/** Installs only the ROM identified by this APK, without touching save files. */
final class BundledRom {
    interface Source { InputStream open() throws IOException; }
    private static final long MAX_BYTES = 64L * 1024 * 1024;

    static void install(File directory, String expectedHash, Source source) throws Exception {
        File rom = new File(directory, "pokeemerald.gba");
        if (rom.isFile() && expectedHash.equals(hash(rom))) return;
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
                if (!expectedHash.equals(hex(digest.digest()))) throw new IOException("La ROM incluida no coincide con esta versión");
                output.getFD().sync();
            }
            Files.move(temporary.toPath(), rom.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } finally {
            Files.deleteIfExists(temporary.toPath());
        }
    }

    private static String hash(File file) throws Exception {
        if (file.length() > MAX_BYTES) return "";
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream input = new FileInputStream(file)) {
            byte[] buffer = new byte[65536];
            int count;
            while ((count = input.read(buffer)) != -1) digest.update(buffer, 0, count);
        }
        return hex(digest.digest());
    }

    private static String hex(byte[] bytes) {
        StringBuilder result = new StringBuilder();
        for (byte value : bytes) result.append(String.format("%02x", value & 255));
        return result.toString();
    }
}
