package io.hoenn.sessions;

import org.junit.Test;
import org.junit.Rule;
import org.junit.rules.TemporaryFolder;
import java.io.*;
import java.nio.file.Files;
import java.security.MessageDigest;
import static org.junit.Assert.*;

public class BundledRomTest {
    @Rule public TemporaryFolder directory = new TemporaryFolder();
    private static String hash(byte[] bytes) throws Exception {
        StringBuilder result = new StringBuilder();
        for (byte value : MessageDigest.getInstance("SHA-256").digest(bytes)) result.append(String.format("%02x", value & 255));
        return result.toString();
    }
    @Test public void installsOnceAndPreservesSaves() throws Exception {
        byte[] rom = {1,2,3};
        File save = new File(directory.getRoot(), "save.sav");
        Files.write(save.toPath(), new byte[]{9});
        BundledRom.install(directory.getRoot(), hash(rom), ()->new ByteArrayInputStream(rom));
        BundledRom.install(directory.getRoot(), hash(rom), ()->{throw new IOException("Asset must not be reopened");});
        assertArrayEquals(rom, Files.readAllBytes(new File(directory.getRoot(), "pokeemerald.gba").toPath()));
        assertArrayEquals(new byte[]{9}, Files.readAllBytes(save.toPath()));
    }
    @Test public void rejectsCorruptAssetWithoutReplacingExistingRom() throws Exception {
        File rom = new File(directory.getRoot(), "pokeemerald.gba");
        Files.write(rom.toPath(), new byte[]{7});
        String expected = hash(new byte[]{1,2,3});
        assertThrows(IOException.class, ()->BundledRom.install(directory.getRoot(), expected, ()->new ByteArrayInputStream(new byte[]{1})));
        assertArrayEquals(new byte[]{7}, Files.readAllBytes(rom.toPath()));
        assertFalse(new File(directory.getRoot(), "bundled-rom.tmp").exists());
    }
    @Test public void cleansUpInterruptedCopy() throws Exception {
        assertThrows(IOException.class, ()->BundledRom.install(directory.getRoot(), hash(new byte[]{1}), ()->new InputStream(){
            @Override public int read() throws IOException {throw new IOException("Interrupted read");}
        }));
        assertFalse(new File(directory.getRoot(), "pokeemerald.gba").exists());
        assertFalse(new File(directory.getRoot(), "bundled-rom.tmp").exists());
    }
}
