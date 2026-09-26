package io.hoenn.sessions;

import android.content.res.AssetManager;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.UUID;
import org.json.JSONObject;

/** Keeps each ROM/manifest pair immutable and switches generations with one marker rename. */
final class RuntimeStore {
    interface ArtifactDownloader {
        void download(String path,File destination,long size,String hash,CloudApi.Progress progress) throws Exception;
    }
    static final class Game {
        final File directory;
        final JSONObject manifest;
        final String romHash, buildId;
        final long sequence;
        final int worldId;
        Game(File directory,JSONObject manifest,long sequence) throws Exception {
            this(directory,manifest,sequence,1);
        }
        Game(File directory,JSONObject manifest,long sequence,int worldId) throws Exception {
            this.directory=directory;this.manifest=manifest;this.sequence=sequence;
            this.worldId=worldId;
            JSONObject build=manifest.getJSONObject("game_build");
            romHash=build.getString("rom_sha256");buildId=build.getString("id");
        }
        int bridgeAddress() throws Exception {return manifest.getJSONObject("net_bridge").getInt("address");}
        int generationAddress() throws Exception {return manifest.getJSONObject("save").getInt("generation_address");}
    }
    private final File runtime;
    RuntimeStore(File root) {runtime=new File(root,"runtime");}
    private BundledRom.VerifiedHashCache romCache() {
        return new BundledRom.VerifiedHashCache(new File(runtime,"verified-rom.properties"));
    }
    private File directory(String id) {return new File(runtime,id);}
    // '@' is excluded by signed release IDs, so transient paths cannot alias a release.
    private File stagingRoot() {return new File(runtime,"@staging");}
    private File damagedRoot() {return new File(runtime,"@damaged");}
    private static void ensureInternalDirectory(File folder) throws IOException {
        if(Files.isSymbolicLink(folder.toPath()) || (!folder.isDirectory() && !folder.mkdirs()))
            throw new IOException("Directorio interno inválido");
    }
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
            if(!ReleaseCatalog.safeId(name) || name.equals("bundled")) continue;
            File envelope=new File(entry,"release-envelope.json");
            if(Files.isSymbolicLink(envelope.toPath()) || !envelope.isFile()) continue;
            try {
                ReleaseCatalog.Release release=verifyStored(CloudApi.bounded(new FileInputStream(envelope),65536),false);
                if(name.equals(release.id) || name.startsWith(release.id+".damaged-"))
                    floor=Math.max(floor,release.sequence);
            } catch(Exception ignored) { /* Unverified files cannot establish a release floor. */ }
        }
        File damaged=damagedRoot();
        if(damaged.isDirectory() && !Files.isSymbolicLink(damaged.toPath())) {
            File[] quarantined=damaged.listFiles();
            if(quarantined!=null) for(File entry:quarantined) {
                if(!entry.isDirectory() || Files.isSymbolicLink(entry.toPath())) continue;
                File envelope=new File(entry,"release-envelope.json");
                if(!envelope.isFile() || Files.isSymbolicLink(envelope.toPath())) continue;
                try {floor=Math.max(floor,verifyStored(CloudApi.bounded(new FileInputStream(envelope),65536),false).sequence);}
                catch(Exception ignored) { /* Unverified files cannot establish a release floor. */ }
            }
        }
        return floor;
    }
    long effectiveSequenceFloor(Game installed) throws Exception {
        return Math.max(signedSequenceFloor(),installed==null?0:installed.sequence);
    }
    private void removeGeneration(File directory) {
        if(Files.isSymbolicLink(directory.toPath()) || !directory.isDirectory()) return;
        File[] children=directory.listFiles();
        if(children==null) return;
        for(File child:children) {
            if(Files.isSymbolicLink(child.toPath())) return;
            if(child.getName().equals("worlds")) {
                if(!validWorldTree(child)) return;
            } else if(!child.isFile() || !(child.getName().equals("pokeemerald.gba")
                || child.getName().equals("bridge_manifest.json") || child.getName().equals("release-envelope.json")
                || child.getName().equals("release_catalog.json"))) return;
        }
        for(File child:children) {
            if(child.isDirectory()) for(File world:child.listFiles()) {
                for(File file:world.listFiles()) file.delete();
                world.delete();
            }
            child.delete();
        }
        directory.delete();
    }
    private static boolean validWorldTree(File tree) {
        if(!tree.isDirectory() || Files.isSymbolicLink(tree.toPath())) return false;
        File[] worlds=tree.listFiles();
        if(worlds==null) return false;
        for(File world:worlds) {
            int id;
            try {id=Integer.parseInt(world.getName());} catch(NumberFormatException error) {return false;}
            if(id<1 || id>65535 || !world.getName().equals(Integer.toString(id))
                || !world.isDirectory() || Files.isSymbolicLink(world.toPath())) return false;
            File[] files=world.listFiles();
            if(files==null) return false;
            for(File file:files) if(!file.isFile() || Files.isSymbolicLink(file.toPath())
                || !(file.getName().equals("game.gba") || file.getName().equals("bridge_manifest.json")
                    || file.getName().equals("player_transfer.json"))) return false;
        }
        return true;
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
    private static ReleaseCatalog.Release verifyStored(byte[] bytes,boolean requireFresh) throws Exception {
        try {
            return ReleaseCatalog.verify(bytes,BuildConfig.RELEASE_KEY_ID,BuildConfig.RELEASE_PUBLIC_KEY_HEX,requireFresh);
        } catch(SecurityException gameError) {
            return ReleaseCatalog.verifyLegacy(bytes,BuildConfig.RELEASE_KEY_ID,BuildConfig.RELEASE_PUBLIC_KEY_HEX,requireFresh);
        }
    }
    private void select(String id) throws Exception {
        File temporary=new File(runtime,"@current.tmp");
        writeSynced(temporary,(id+"\n").getBytes(StandardCharsets.UTF_8));
        Files.move(temporary.toPath(),marker().toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
    }
    void prepareBundled(AssetManager assets) throws Exception {
        if(!runtime.isDirectory() && !runtime.mkdirs()) throw new IOException("No se pudo preparar el juego");
        File bundled=directory("bundled");
        if(!bundled.isDirectory() && !bundled.mkdirs()) throw new IOException("No se pudo preparar el juego");
        BundledRom.install(bundled,BuildConfig.ROM_SHA256,()->assets.open("pokeemerald.gba"),romCache());
        try(InputStream input=assets.open("bridge_manifest.json")) {
            writeSynced(new File(bundled,"bridge_manifest.json"),CloudApi.bounded(input,1024*1024));
        }
        ReleaseCatalog.verifiedManifest(new File(bundled,"bridge_manifest.json"),BuildConfig.ROM_SHA256);
        if(!marker().isFile()) select("bundled");
    }
    Game current() throws Exception {return current(true);}
    Game current(int worldId) throws Exception {return currentWorld(worldId,true);}
    private Game currentWorld(int worldId,boolean requireFresh) throws Exception {
        if(worldId<1 || worldId>65535) throw new SecurityException("Mundo inválido");
        String id=selectedId();
        if(id.equals("bundled")) {
            if(worldId!=1) throw new SecurityException("Mundo no instalado");
            return current(requireFresh);
        }
        File selected=directory(id);
        byte[] signed=CloudApi.bounded(new FileInputStream(new File(selected,"release-envelope.json")),65536);
        ReleaseCatalog.Release release=verifyStored(signed,requireFresh);
        if(!id.equals(release.id)) throw new SecurityException("Identidad de versión local inválida");
        ReleaseCatalog.World world=release.worlds.get(worldId);
        if(world==null) {
            if(worldId==1 && release.worlds.isEmpty()) return current(requireFresh);
            throw new SecurityException("Mundo no instalado");
        }
        ReleaseCatalog.verifiedWorldCatalog(new File(selected,"release_catalog.json"),release);
        return verifyWorld(selected,world,release.sequence);
    }
    private void pruneQuarantine(long selectedSequence) {
        File quarantine=damagedRoot();
        if(Files.isSymbolicLink(quarantine.toPath()) || !quarantine.isDirectory()) return;
        File[] entries=quarantine.listFiles();
        if(entries==null) return;
        File highestEntry=null;
        long highestSequence=selectedSequence;
        for(File entry:entries) {
            if(Files.isSymbolicLink(entry.toPath()) || !entry.isDirectory()) continue;
            File envelope=new File(entry,"release-envelope.json");
            if(envelope.isFile() && !Files.isSymbolicLink(envelope.toPath())) {
                try {
                    long sequence=verifyStored(CloudApi.bounded(new FileInputStream(envelope),65536),false).sequence;
                    if(sequence>highestSequence) {highestSequence=sequence;highestEntry=entry;}
                } catch(Exception ignored) { /* Unverified quarantine cannot raise the rollback floor. */ }
            }
        }
        for(File entry:entries) if(!entry.equals(highestEntry)) removeGeneration(entry);
        quarantine.delete(); // Succeeds only when every safe, obsolete entry was removed.
    }
    private Game verifyWorld(File selected,ReleaseCatalog.World world,long sequence) throws Exception {
        File folder=new File(new File(selected,"worlds"),Integer.toString(world.id));
        File rom=new File(folder,"game.gba");
        File manifest=new File(folder,"bridge_manifest.json");
        File transfer=new File(folder,"player_transfer.json");
        if(Files.isSymbolicLink(folder.toPath()) || Files.isSymbolicLink(rom.toPath())
            || Files.isSymbolicLink(manifest.toPath()) || Files.isSymbolicLink(transfer.toPath()))
            throw new SecurityException("Mundo local inválido");
        if(!romCache().verify(rom,world.rom.size,world.rom.sha256)) throw new SecurityException("ROM de mundo modificada");
        ReleaseCatalog.verifyFile(manifest,world.manifest);
        ReleaseCatalog.verifyFile(transfer,world.playerTransfer);
        return new Game(folder,ReleaseCatalog.verifiedManifest(manifest,world.rom.sha256),sequence,world.id);
    }
    private Game current(boolean requireFresh) throws Exception {
        String id=selectedId();
        File selected=directory(id);
        File rom=new File(selected,"pokeemerald.gba");
        File manifest=new File(selected,"bridge_manifest.json");
        long sequence=0;
        if(id.equals("bundled")) {
            if(rom.length()==0 || rom.length()>64L*1024*1024) throw new SecurityException("ROM incluida inválida");
            if(!romCache().verify(rom,rom.length(),BuildConfig.ROM_SHA256)) throw new SecurityException("ROM incluida modificada");
            JSONObject parsed=ReleaseCatalog.verifiedManifest(manifest,BuildConfig.ROM_SHA256);
            return new Game(selected,parsed,sequence);
        }
        byte[] signed=CloudApi.bounded(new FileInputStream(new File(selected,"release-envelope.json")),65536);
        ReleaseCatalog.Release release=verifyStored(signed,requireFresh);
        if(!id.equals(release.id)) throw new SecurityException("Identidad de versión local inválida");
        if(!romCache().verify(rom,release.rom.size,release.rom.sha256)) throw new SecurityException("Archivo de versión modificado");
        ReleaseCatalog.verifyFile(manifest,release.manifest);
        JSONObject parsed=ReleaseCatalog.verifiedManifest(manifest,release.rom.sha256);
        verifyAllWorlds(selected,release);
        return new Game(selected,parsed,release.sequence);
    }
    private void verifyAllWorlds(File selected,ReleaseCatalog.Release release) throws Exception {
        if(release.regionCatalog==null) return;
        File catalog=new File(selected,"release_catalog.json");
        if(Files.isSymbolicLink(catalog.toPath())) throw new SecurityException("Catálogo local inválido");
        ReleaseCatalog.verifiedWorldCatalog(catalog,release);
        for(ReleaseCatalog.World world:release.worlds.values()) verifyWorld(selected,world,release.sequence);
    }
    Game ensureLatest(CloudApi api) throws Exception {
        return ensureLatest(api,null);
    }
    Game ensureLatest(CloudApi api,CloudApi.Progress progress) throws Exception {
        byte[] bytes;
        try {bytes=api.request("/v1/releases/game/latest",null,65536);}
        catch(IOException error) {return current();}
        ReleaseCatalog.Release latest=ReleaseCatalog.verify(bytes,BuildConfig.RELEASE_KEY_ID,BuildConfig.RELEASE_PUBLIC_KEY_HEX);
        if(latest.id.equals("bundled")) throw new SecurityException("Identidad de versión reservada");
        Game installed;
        try {installed=current(false);} catch(Exception invalidSelection) {installed=null;}
        long floor=effectiveSequenceFloor(installed);
        if(latest.sequence<floor) throw new SecurityException("El servidor ofrece una versión anterior");
        if(installed!=null && directory(latest.id).equals(installed.directory)
            && installed.sequence==latest.sequence
            && java.util.Arrays.equals(
                CloudApi.bounded(new FileInputStream(new File(installed.directory,"release-envelope.json")),65536),bytes))
            return installed;
        File target=directory(latest.id);
        if(target.exists()) {
            try {
                if(!romCache().verify(new File(target,"pokeemerald.gba"),latest.rom.size,latest.rom.sha256))
                    throw new SecurityException("Archivo de versión modificado");
                ReleaseCatalog.verifyFile(new File(target,"bridge_manifest.json"),latest.manifest);
                JSONObject parsed=ReleaseCatalog.verifiedManifest(new File(target,"bridge_manifest.json"),latest.rom.sha256);
                verifyAllWorlds(target,latest);
                byte[] stored=CloudApi.bounded(new FileInputStream(new File(target,"release-envelope.json")),65536);
                if(!java.util.Arrays.equals(stored,bytes)) throw new SecurityException("Versión local distinta del servidor");
                select(latest.id);
                pruneQuarantine(latest.sequence);
                if(installed!=null) pruneGenerations(latest.id,installed.directory.getName());
                return new Game(target,parsed,latest.sequence);
            } catch(Exception invalidTarget) {
                if(Files.isSymbolicLink(target.toPath()) || !target.isDirectory()) throw invalidTarget;
            }
        }
        return installRelease(api::download,bytes,latest,progress,installed,target);
    }
    // Keep staging separate from selection so a rejected world never replaces the active release.
    Game installRelease(ArtifactDownloader downloader,byte[] bytes,ReleaseCatalog.Release latest,
                        CloudApi.Progress progress,Game installed,File target) throws Exception {
        File stagingParent=stagingRoot();
        ensureInternalDirectory(stagingParent);
        File staging=new File(stagingParent,latest.id);
        if(Files.isSymbolicLink(staging.toPath()) || (!staging.isDirectory() && !staging.mkdirs()))
            throw new IOException("No se pudo preparar la descarga");
        boolean keepPartial=false;
        try {
            File rom=new File(staging,"pokeemerald.gba");
            File manifest=new File(staging,"bridge_manifest.json");
            if(Files.isSymbolicLink(rom.toPath()) || Files.isSymbolicLink(manifest.toPath()))
                throw new SecurityException("Descarga local inválida");
            long total=latest.rom.size+latest.manifest.size;
            downloader.download("/v1/releases/game/"+latest.id+"/artifacts/rom",rom,latest.rom.size,latest.rom.sha256,
                progress==null?null:(received,ignored)->progress.onProgress(received,total));
            downloader.download("/v1/releases/game/"+latest.id+"/artifacts/compatibility-manifest",manifest,latest.manifest.size,latest.manifest.sha256,
                progress==null?null:(received,ignored)->progress.onProgress(latest.rom.size+received,total));
            JSONObject parsed=ReleaseCatalog.verifiedManifest(manifest,latest.rom.sha256);
            if(latest.regionCatalog!=null) {
                File catalog=new File(staging,"release_catalog.json");
                if(Files.isSymbolicLink(catalog.toPath())) throw new SecurityException("Catálogo local inválido");
                downloader.download("/v1/releases/game/"+latest.id+"/artifacts/region-catalog",catalog,
                    latest.regionCatalog.size,latest.regionCatalog.sha256,null);
                ReleaseCatalog.verifiedWorldCatalog(catalog,latest);
                for(ReleaseCatalog.World world:latest.worlds.values()) {
                    File folder=new File(new File(staging,"worlds"),Integer.toString(world.id));
                    if(Files.isSymbolicLink(folder.getParentFile().toPath()) || Files.isSymbolicLink(folder.toPath()))
                        throw new SecurityException("Mundo local inválido");
                    if(!folder.isDirectory() && !folder.mkdirs()) throw new IOException("No se pudo preparar el mundo");
                    for(String name:new String[]{"game.gba","bridge_manifest.json","player_transfer.json"})
                        if(Files.isSymbolicLink(new File(folder,name).toPath())) throw new SecurityException("Mundo local inválido");
                    String url="/v1/releases/game/"+latest.id+"/artifacts/world-"+world.id+"-";
                    downloader.download(url+"rom",new File(folder,"game.gba"),world.rom.size,world.rom.sha256,null);
                    downloader.download(url+"compatibility",new File(folder,"bridge_manifest.json"),world.manifest.size,world.manifest.sha256,null);
                    downloader.download(url+"player-transfer",new File(folder,"player_transfer.json"),world.playerTransfer.size,world.playerTransfer.sha256,null);
                }
                verifyAllWorlds(staging,latest);
            }
            writeSynced(new File(staging,"release-envelope.json"),bytes);
            if(target.exists()) {
                File quarantine=damagedRoot();
                ensureInternalDirectory(quarantine);
                File damaged=new File(quarantine,latest.id+"-"+UUID.randomUUID());
                Files.move(target.toPath(),damaged.toPath(),StandardCopyOption.ATOMIC_MOVE);
            }
            Files.move(staging.toPath(),target.toPath(),StandardCopyOption.ATOMIC_MOVE);
            select(latest.id);
            pruneQuarantine(latest.sequence);
            if(installed!=null) pruneGenerations(latest.id,installed.directory.getName());
            return new Game(target,parsed,latest.sequence);
        } catch(IOException interruptedDownload) {
            keepPartial=true;
            throw interruptedDownload;
        } finally {
            if(!keepPartial && staging.exists()) {
                removeGeneration(staging);
            }
        }
    }
}
