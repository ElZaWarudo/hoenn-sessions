package io.hoenn.sessions;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;
import static org.junit.Assert.*;

public class RuntimeStoreTest {
    private static byte[] bytes(String value) {return value.getBytes(StandardCharsets.UTF_8);}
    private static ReleaseCatalog.Artifact artifact(byte[] contents) throws Exception {
        return new ReleaseCatalog.Artifact(contents.length,
            ReleaseCatalog.hex(MessageDigest.getInstance("SHA-256").digest(contents)));
    }

    @Test public void rejectedNestedWorldDownloadKeepsSelectedReleaseAndCleansStaging() throws Exception {
        assertRollback("foo.partial");
        assertRollback("foo.damaged-12345678-1234-1234-1234-123456789abc");
    }

    private static void assertRollback(String selectedId) throws Exception {
        assertTrue(ReleaseCatalog.safeId(selectedId));
        File root=Files.createTempDirectory("runtime-world-rollback-").toFile();
        try {
            File runtime=new File(root,"runtime");
            assertTrue(runtime.mkdir());
            Files.writeString(new File(runtime,"current").toPath(),selectedId+"\n");
            File old=new File(runtime,selectedId);
            assertTrue(old.mkdir());
            Files.writeString(new File(old,"keep").toPath(),"existing data");

            byte[] rom=bytes("main rom"), otherRom=bytes("cormoria rom"), transfer=bytes("descriptor");
            ReleaseCatalog.Artifact mainRom=artifact(rom), cormoriaRom=artifact(otherRom);
            ReleaseCatalog.Artifact playerTransfer=artifact(transfer);
            byte[] mainManifest=manifest(mainRom.sha256), cormoriaManifest=manifest(cormoriaRom.sha256);
            ReleaseCatalog.Artifact mainBridge=artifact(mainManifest), cormoriaBridge=artifact(cormoriaManifest);
            LinkedHashMap<Integer,ReleaseCatalog.World> worlds=new LinkedHashMap<>();
            worlds.put(1,new ReleaseCatalog.World(1,mainRom,mainBridge,playerTransfer));
            worlds.put(2,new ReleaseCatalog.World(2,cormoriaRom,cormoriaBridge,playerTransfer));
            JSONArray entries=new JSONArray();
            for(ReleaseCatalog.World world:worlds.values()) {
                String prefix="worlds/"+world.id+"/";
                entries.put(new JSONObject().put("world_id",world.id)
                    .put("rom_path",prefix+"game.gba").put("rom_sha256",world.rom.sha256)
                    .put("bridge_path",prefix+"bridge_manifest.json").put("bridge_sha256",world.manifest.sha256)
                    .put("player_transfer_path",prefix+"player_transfer.json")
                    .put("player_transfer_sha256",world.playerTransfer.sha256));
            }
            byte[] catalog=bytes(new JSONObject().put("schema_version",1).put("worlds",entries).toString());
            ReleaseCatalog.Release release=new ReleaseCatalog.Release("foo",2,mainRom,mainBridge,
                artifact(catalog),worlds);
            Map<String,byte[]> downloads=new HashMap<>();
            downloads.put("rom",rom);
            downloads.put("compatibility-manifest",mainManifest);
            downloads.put("region-catalog",catalog);
            downloads.put("world-1-rom",rom);
            downloads.put("world-1-compatibility",mainManifest);
            downloads.put("world-1-player-transfer",transfer);
            downloads.put("world-2-rom",otherRom);
            downloads.put("world-2-compatibility",cormoriaManifest);
            downloads.put("world-2-player-transfer",transfer);
            RuntimeStore.ArtifactDownloader interrupted=(path,destination,size,hash,progress)->{
                    String id=path.substring(path.lastIndexOf('/')+1);
                    byte[] data=downloads.get(id);
                    assertNotNull(id,data);
                    assertEquals(size,data.length);
                    assertEquals(hash,artifact(data).sha256);
                    Files.write(destination.toPath(),data);
                    if(id.equals("world-2-player-transfer")) throw new SecurityException("rejected world");
            };
            RuntimeStore store=new RuntimeStore(root);
            try {store.installRelease(interrupted,bytes("signed envelope"),release,null,null,
                new File(runtime,release.id));fail("Installed a rejected world");}
            catch(SecurityException expected) { }
            assertEquals(selectedId+"\n",Files.readString(new File(runtime,"current").toPath()));
            assertEquals("existing data",Files.readString(new File(old,"keep").toPath()));
            assertFalse(new File(runtime,"foo").exists());
            assertFalse(new File(new File(runtime,"@staging"),"foo").exists());
        } finally {
            try(java.util.stream.Stream<java.nio.file.Path> paths=Files.walk(root.toPath())) {
                paths.sorted(java.util.Comparator.reverseOrder()).forEach(path->path.toFile().delete());
            }
        }
    }

    @Test public void repeatedSameIdReplacementPrunesQuarantineButKeepsSelectedGeneration() throws Exception {
        File root=Files.createTempDirectory("runtime-world-replace-").toFile();
        try {
            File runtime=new File(root,"runtime");
            assertTrue(runtime.mkdir());
            File target=new File(runtime,"foo");
            assertTrue(target.mkdir());
            Files.writeString(new File(runtime,"current").toPath(),"foo\n");
            Files.writeString(new File(target,"pokeemerald.gba").toPath(),"old ROM");
            Files.writeString(new File(target,"bridge_manifest.json").toPath(),"old manifest");
            Files.writeString(new File(target,"release-envelope.json").toPath(),"old envelope");
            RuntimeStore store=new RuntimeStore(root);
            for(int sequence:new int[]{2,3}) {
                byte[] rom=bytes("ROM sequence "+sequence);
                ReleaseCatalog.Artifact romArtifact=artifact(rom);
                byte[] bridge=manifest(romArtifact.sha256);
                ReleaseCatalog.Artifact bridgeArtifact=artifact(bridge);
                byte[] otherRom=bytes("Cormoria ROM sequence "+sequence);
                ReleaseCatalog.Artifact otherRomArtifact=artifact(otherRom);
                byte[] otherBridge=manifest(otherRomArtifact.sha256);
                ReleaseCatalog.Artifact otherBridgeArtifact=artifact(otherBridge);
                byte[] transfer=bytes("shared descriptor");
                ReleaseCatalog.Artifact transferArtifact=artifact(transfer);
                LinkedHashMap<Integer,ReleaseCatalog.World> worlds=new LinkedHashMap<>();
                worlds.put(1,new ReleaseCatalog.World(1,romArtifact,bridgeArtifact,transferArtifact));
                worlds.put(2,new ReleaseCatalog.World(2,otherRomArtifact,otherBridgeArtifact,transferArtifact));
                JSONArray entries=new JSONArray();
                for(ReleaseCatalog.World world:worlds.values()) {
                    String prefix="worlds/"+world.id+"/";
                    entries.put(new JSONObject().put("world_id",world.id)
                        .put("rom_path",prefix+"game.gba").put("rom_sha256",world.rom.sha256)
                        .put("bridge_path",prefix+"bridge_manifest.json").put("bridge_sha256",world.manifest.sha256)
                        .put("player_transfer_path",prefix+"player_transfer.json")
                        .put("player_transfer_sha256",world.playerTransfer.sha256));
                }
                byte[] catalog=bytes(new JSONObject().put("schema_version",1).put("worlds",entries).toString());
                ReleaseCatalog.Release release=new ReleaseCatalog.Release("foo",sequence,romArtifact,
                    bridgeArtifact,artifact(catalog),worlds);
                Map<String,byte[]> downloads=new HashMap<>();
                downloads.put("rom",rom);
                downloads.put("compatibility-manifest",bridge);
                downloads.put("region-catalog",catalog);
                downloads.put("world-1-rom",rom);
                downloads.put("world-1-compatibility",bridge);
                downloads.put("world-1-player-transfer",transfer);
                downloads.put("world-2-rom",otherRom);
                downloads.put("world-2-compatibility",otherBridge);
                downloads.put("world-2-player-transfer",transfer);
                RuntimeStore.ArtifactDownloader downloader=(path,destination,size,hash,progress)->{
                    byte[] content=downloads.get(path.substring(path.lastIndexOf('/')+1));
                    assertNotNull(content);
                    assertEquals(hash,artifact(content).sha256);
                    Files.write(destination.toPath(),content);
                };
                byte[] envelope=bytes("envelope sequence "+sequence);
                RuntimeStore.Game selected=store.installRelease(downloader,envelope,release,null,null,target);
                assertEquals(sequence,selected.sequence);
                assertArrayEquals(envelope,Files.readAllBytes(new File(target,"release-envelope.json").toPath()));
                assertArrayEquals(rom,Files.readAllBytes(new File(target,"pokeemerald.gba").toPath()));
                assertArrayEquals(otherRom,Files.readAllBytes(new File(target,"worlds/2/game.gba").toPath()));
                assertEquals("foo\n",Files.readString(new File(runtime,"current").toPath()));
                assertEquals(sequence,store.effectiveSequenceFloor(selected));
                File quarantine=new File(runtime,"@damaged");
                assertTrue(!quarantine.exists() || quarantine.list().length==0);
            }
        } finally {
            try(java.util.stream.Stream<java.nio.file.Path> paths=Files.walk(root.toPath())) {
                paths.sorted(java.util.Comparator.reverseOrder()).forEach(path->path.toFile().delete());
            }
        }
    }

    private static byte[] manifest(String romHash) throws Exception {
        return bytes(new JSONObject().put("schema_version",4)
            .put("game_build",new JSONObject().put("id","test").put("rom_sha256",romHash))
            .put("net_bridge",new JSONObject().put("abi_version",1).put("game_protocol_version",1))
            .toString());
    }
}
