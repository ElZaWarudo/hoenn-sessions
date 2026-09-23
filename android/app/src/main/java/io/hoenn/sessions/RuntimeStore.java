package io.hoenn.sessions;

import android.content.res.AssetManager;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.UUID;
import org.json.JSONObject;

/** Keeps each ROM/manifest pair immutable and switches generations with one marker rename. */
final class RuntimeStore {
    static final class Game {
        final File directory;
        final JSONObject manifest;
        final String romHash, buildId;
        final long sequence;
        Game(File directory,JSONObject manifest,long sequence) throws Exception {
            this.directory=directory;this.manifest=manifest;this.sequence=sequence;
            JSONObject build=manifest.getJSONObject("game_build");
            romHash=build.getString("rom_sha256");buildId=build.getString("id");
        }
        int bridgeAddress() throws Exception {return manifest.getJSONObject("net_bridge").getInt("address");}
        int generationAddress() throws Exception {return manifest.getJSONObject("save").getInt("generation_address");}
    }
    private final File runtime;
    RuntimeStore(File root) {runtime=new File(root,"runtime");}
    private File directory(String id) {return new File(runtime,id);}
    private File marker() {return new File(runtime,"current");}
    private String selectedId() throws Exception {
        byte[] markerBytes=CloudApi.bounded(new FileInputStream(marker()),130);
        int end=markerBytes.length;
        while(end>0 && (markerBytes[end-1]=='\r' || markerBytes[end-1]=='\n')) end--;
        String id=new String(markerBytes,0,end,StandardCharsets.US_ASCII);
        if(!ReleaseCatalog.safeId(id)) throw new SecurityException("Versión local inválida");
        return id;
    }
    private long signedSequenceFloor() throws Exception {
        long floor=0;
        File[] entries=runtime.listFiles();
        if(entries==null) return floor;
        for(File entry:entries) {
            if(Files.isSymbolicLink(entry.toPath()) || !entry.isDirectory()) continue;
            String name=entry.getName();
            if(!ReleaseCatalog.safeId(name) || name.equals("bundled") || name.contains(".partial-")) continue;
            File envelope=new File(entry,"release-envelope.json");
            if(Files.isSymbolicLink(envelope.toPath()) || !envelope.isFile()) continue;
            try {
                ReleaseCatalog.Release release=ReleaseCatalog.verify(
                    CloudApi.bounded(new FileInputStream(envelope),65536),
                    BuildConfig.RELEASE_KEY_ID,BuildConfig.RELEASE_PUBLIC_KEY_HEX,false);
                if(name.equals(release.id) || name.startsWith(release.id+".damaged-"))
                    floor=Math.max(floor,release.sequence);
            } catch(Exception ignored) { /* Unverified files cannot establish a release floor. */ }
        }
        return floor;
    }
    private void removeGeneration(File directory) {
        if(Files.isSymbolicLink(directory.toPath()) || !directory.isDirectory()) return;
        File[] children=directory.listFiles();
        if(children==null) return;
        for(File child:children) {
            if(!child.isFile() || Files.isSymbolicLink(child.toPath())
                || !(child.getName().equals("pokeemerald.gba") || child.getName().equals("bridge_manifest.json")
                    || child.getName().equals("release-envelope.json"))) return;
        }
        for(File child:children) child.delete();
        directory.delete();
    }
    private void cleanStaging() {
        File[] entries=runtime.listFiles();
        if(entries!=null) for(File entry:entries)
            if(entry.getName().matches("[A-Za-z0-9._-]+\\.partial-[0-9a-f-]{36}")) removeGeneration(entry);
    }
    private void pruneGenerations(String selected,String previous) {
        File[] entries=runtime.listFiles();
        if(entries!=null) for(File entry:entries) {
            String name=entry.getName();
            if(ReleaseCatalog.safeId(name) && !name.equals(selected) && !name.equals(previous)) removeGeneration(entry);
        }
    }
    private static void writeSynced(File file,byte[] bytes) throws Exception {
        try(FileOutputStream output=new FileOutputStream(file)) {output.write(bytes);output.getFD().sync();}
    }
    private void select(String id) throws Exception {
        File temporary=new File(runtime,"current.tmp");
        writeSynced(temporary,(id+"\n").getBytes(StandardCharsets.UTF_8));
        Files.move(temporary.toPath(),marker().toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
    }
    void prepareBundled(AssetManager assets) throws Exception {
        if(!runtime.isDirectory() && !runtime.mkdirs()) throw new IOException("No se pudo preparar el juego");
        File bundled=directory("bundled");
        if(!bundled.isDirectory() && !bundled.mkdirs()) throw new IOException("No se pudo preparar el juego");
        BundledRom.install(bundled,BuildConfig.ROM_SHA256,()->assets.open("pokeemerald.gba"));
        try(InputStream input=assets.open("bridge_manifest.json")) {
            writeSynced(new File(bundled,"bridge_manifest.json"),CloudApi.bounded(input,1024*1024));
        }
        ReleaseCatalog.verifiedManifest(new File(bundled,"bridge_manifest.json"),BuildConfig.ROM_SHA256);
        if(!marker().isFile()) select("bundled");
    }
    Game current() throws Exception {return current(true);}
    private Game current(boolean requireFresh) throws Exception {
        String id=selectedId();
        File selected=directory(id);
        File rom=new File(selected,"pokeemerald.gba");
        File manifest=new File(selected,"bridge_manifest.json");
        long sequence=0;
        if(id.equals("bundled")) {
            if(rom.length()==0 || rom.length()>64L*1024*1024) throw new SecurityException("ROM incluida inválida");
            MessageDigest digest=MessageDigest.getInstance("SHA-256");
            try(InputStream input=new FileInputStream(rom)) {byte[] bytes=new byte[65536];int n;while((n=input.read(bytes))!=-1)digest.update(bytes,0,n);}
            if(!BuildConfig.ROM_SHA256.equals(ReleaseCatalog.hex(digest.digest()))) throw new SecurityException("ROM incluida modificada");
            JSONObject parsed=ReleaseCatalog.verifiedManifest(manifest,BuildConfig.ROM_SHA256);
            return new Game(selected,parsed,sequence);
        }
        byte[] signed=CloudApi.bounded(new FileInputStream(new File(selected,"release-envelope.json")),65536);
        ReleaseCatalog.Release release=ReleaseCatalog.verify(signed,BuildConfig.RELEASE_KEY_ID,BuildConfig.RELEASE_PUBLIC_KEY_HEX,requireFresh);
        if(!id.equals(release.id)) throw new SecurityException("Identidad de versión local inválida");
        ReleaseCatalog.verifyFile(rom,release.rom);
        ReleaseCatalog.verifyFile(manifest,release.manifest);
        JSONObject parsed=ReleaseCatalog.verifiedManifest(manifest,release.rom.sha256);
        return new Game(selected,parsed,release.sequence);
    }
    Game ensureLatest(CloudApi api) throws Exception {
        cleanStaging();
        byte[] bytes;
        try {bytes=api.request("/v1/releases/windows-x86_64/latest",null,65536);}
        catch(IOException error) {return current();}
        ReleaseCatalog.Release latest=ReleaseCatalog.verify(bytes,BuildConfig.RELEASE_KEY_ID,BuildConfig.RELEASE_PUBLIC_KEY_HEX);
        if(latest.id.equals("bundled")) throw new SecurityException("Identidad de versión reservada");
        Game installed;
        try {installed=current(false);} catch(Exception invalidSelection) {installed=null;}
        long floor=Math.max(signedSequenceFloor(),installed==null?0:installed.sequence);
        if(latest.sequence<floor) throw new SecurityException("El servidor ofrece una versión anterior");
        if(installed!=null && directory(latest.id).equals(installed.directory)
            && installed.sequence==latest.sequence
            && java.util.Arrays.equals(
                CloudApi.bounded(new FileInputStream(new File(installed.directory,"release-envelope.json")),65536),bytes))
            return installed;
        File target=directory(latest.id);
        if(target.exists()) {
            try {
                ReleaseCatalog.verifyFile(new File(target,"pokeemerald.gba"),latest.rom);
                ReleaseCatalog.verifyFile(new File(target,"bridge_manifest.json"),latest.manifest);
                JSONObject parsed=ReleaseCatalog.verifiedManifest(new File(target,"bridge_manifest.json"),latest.rom.sha256);
                byte[] stored=CloudApi.bounded(new FileInputStream(new File(target,"release-envelope.json")),65536);
                if(!java.util.Arrays.equals(stored,bytes)) throw new SecurityException("Versión local distinta del servidor");
                select(latest.id);
                if(installed!=null) pruneGenerations(latest.id,installed.directory.getName());
                return new Game(target,parsed,latest.sequence);
            } catch(Exception invalidTarget) {
                if(Files.isSymbolicLink(target.toPath()) || !target.isDirectory()) throw invalidTarget;
            }
        }
        File staging=directory(latest.id+".partial-"+UUID.randomUUID());
        if(!staging.mkdirs()) throw new IOException("No se pudo preparar la descarga");
        try {
            File rom=new File(staging,"pokeemerald.gba");
            File manifest=new File(staging,"bridge_manifest.json");
            api.download("/v1/releases/"+latest.id+"/artifacts/rom",rom,latest.rom.size,latest.rom.sha256);
            api.download("/v1/releases/"+latest.id+"/artifacts/compatibility-manifest",manifest,latest.manifest.size,latest.manifest.sha256);
            JSONObject parsed=ReleaseCatalog.verifiedManifest(manifest,latest.rom.sha256);
            writeSynced(new File(staging,"release-envelope.json"),bytes);
            if(target.exists()) {
                File damaged=directory(latest.id+".damaged-"+UUID.randomUUID());
                Files.move(target.toPath(),damaged.toPath(),StandardCopyOption.ATOMIC_MOVE);
            }
            Files.move(staging.toPath(),target.toPath(),StandardCopyOption.ATOMIC_MOVE);
            select(latest.id);
            if(installed!=null) pruneGenerations(latest.id,installed.directory.getName());
            return new Game(target,parsed,latest.sequence);
        } finally {
            if(staging.exists()) {
                for(File child:staging.listFiles()==null?new File[0]:staging.listFiles()) child.delete();
                staging.delete();
            }
        }
    }
}
