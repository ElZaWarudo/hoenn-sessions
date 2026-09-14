package io.hoenn.sessions;

import android.app.Activity;
import android.content.Intent;
import android.graphics.*;
import android.media.*;
import android.os.*;
import android.text.InputType;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.*;
import java.security.MessageDigest;
import java.util.concurrent.*;
import org.json.JSONObject;

public final class MainActivity extends Activity {
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final CloudApi api=new CloudApi();
    private TextView status;
    private LinearLayout layout;
    private EditText user,password,invitation;
    private GameView game;
    private volatile BridgeConnection connection;
    private volatile boolean cooperative;
    private volatile boolean pendingStart,destroyed;
    private final Handler hostHandler=new Handler(Looper.getMainLooper());
    private final Runnable hostPoll=new Runnable(){public void run(){
        try{pollSession();}catch(Exception e){status.setText("Error de sesión: "+e.getMessage());NativeSession.stop();closeCore();}
        if(destroyed && !pendingStart && !NativeSession.isActive()){game.stop();closeCore();return;}
        hostHandler.postDelayed(this,50);
    }};
    private volatile boolean resumed;
    private interface Work {String run() throws Exception;}
    private void work(Work work) {
        worker.execute(()->{try{String result=work.run(); runOnUiThread(()->status.setText(result));}catch(Exception e){runOnUiThread(()->status.setText(e.getClass().getSimpleName()+": "+e.getMessage()));}});
    }
    private void button(String label,Runnable action){Button b=new Button(this);b.setText(label);b.setOnClickListener(v->action.run());layout.addView(b);}
    private EditText field(String label,boolean secret){EditText e=new EditText(this);e.setHint(label);e.setSingleLine();e.setSaveEnabled(false);e.setInputType(secret?129:InputType.TYPE_CLASS_TEXT);layout.addView(e);return e;}
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);
        getWindow().setFlags(WindowManager.LayoutParams.FLAG_SECURE,WindowManager.LayoutParams.FLAG_SECURE);
        ScrollView scroll=new ScrollView(this);layout=new LinearLayout(this);layout.setOrientation(LinearLayout.VERTICAL);layout.setPadding(24,48,24,32);scroll.addView(layout);setContentView(scroll);
        TextView title=new TextView(this);title.setText("HOENN SESSIONS\nAndroid · núcleo GBA de prueba");title.setTextSize(23);layout.addView(title);
        TextView scope=new TextView(this);scope.setText("mGBA 0.10.5 · ciclo Rust cooperativo en validación. Guarda desde el menú del juego antes de cerrar.\nServidor: "+CloudApi.BASE+"\nClave fijada: pilot-v1");layout.addView(scope);
        status=new TextView(this);status.setTag("session-status");status.setText("Sin conexión comprobada");status.setPadding(0,20,0,20);layout.addView(status);
        button("Comprobar HTTPS",()->work(()->{api.health();return "HTTPS válido · servidor preparado. Firma pilot-v1 pendiente de un paquete firmado.";}));
        user=field("Usuario de prueba",false);password=field("Contraseña",true);invitation=field("Invitación (solo registro)",true);
        button("Registrar cuenta",()->{String u=user.getText().toString(),p=password.getText().toString(),i=invitation.getText().toString();invitation.setText("");work(()->{api.register(u,p,i);return "Cuenta registrada. Ya puedes iniciar sesión.";});});
        button("Iniciar sesión y jugar / reanudar",()->{
            if(pendingStart || NativeSession.isActive()){status.setText("Ya hay una sesión activa o cerrándose.");return;}
            pendingStart=true;
            String u=user.getText().toString(),p=password.getText().toString();password.setText("");
            work(()->{try{
                if(destroyed || !resumed)return "Inicio cancelado al salir de la aplicación.";
                verifyCore();api.logout();copyManifest();closeCore();
                if(!NativeSession.start(getFilesDir().getCanonicalPath(),u,p))throw new IOException("Ya hay una sesión activa");
                if(destroyed || !resumed)NativeSession.stop();
                return "Adquiriendo lease y verificando partida…";
            }finally{pendingStart=false;}});
        });
        button("Reconectar desde último guardado cloud",()->{
            status.setText(NativeSession.reconnect()?"Cerrando el bridge y solicitando nuevo epoch…":"Necesitas una sesión activa y un guardado ya aceptado por el servidor.");
        });
        button("Cerrar sesión",()->{NativeSession.stop();status.setText("Cierre solicitado; espera la confirmación.");});
        button("Importar ROM compatible",()->{
            if(pendingStart || NativeSession.isActive()){status.setText("Cierra la sesión antes de importar una ROM.");return;}
            Intent i=new Intent(Intent.ACTION_OPEN_DOCUMENT);i.setType("*/*");i.addCategory(Intent.CATEGORY_OPENABLE);startActivityForResult(i,1);
        });
        TextView expected=new TextView(this);expected.setText("ROM requerida: pokeemerald.gba\nSHA-256: "+PinnedIdentity.ROM_HASH+"\nNo se incluye ROM ni BIOS.");layout.addView(expected);
        game=new GameView();game.setTag("gba-frame");layout.addView(game,new LinearLayout.LayoutParams(-1,480));
        LinearLayout keys=new LinearLayout(this);keys.setOrientation(LinearLayout.VERTICAL);layout.addView(keys);
        addKeys(keys,new String[]{"↑","↓","←","→"},new int[]{64,128,32,16});
        addKeys(keys,new String[]{"A","B","L","R","START","SELECT"},new int[]{1,2,512,256,8,4});
        button("Cerrar partida (guarda antes en el juego)",()->{if(pendingStart || NativeSession.isActive()){status.setText("Cerrando y comprobando checkpoint…");NativeSession.stop();}else work(()->{closeCore();return "Juego local detenido.";});});
        hostHandler.post(hostPoll);
    }
    private void copyManifest() throws Exception {try(InputStream in=getAssets().open("bridge_manifest.json");OutputStream out=new FileOutputStream(new File(getFilesDir(),"bridge_manifest.json"))){byte[] b=new byte[8192];int n;while((n=in.read(b))!=-1)out.write(b,0,n);}}
    private void verifyCore() throws IOException {
        if(!"0.10.5|26b7884bc25a5933960f3cdcd98bac1ae14d42e2".equals(NativeCore.identity()))throw new IOException("Identidad del núcleo mGBA no admitida");
    }
    private void closeCore(){
        synchronized(NativeCore.class){try{if(connection!=null)connection.close();}finally{connection=null;NativeCore.close();cooperative=false;}}
        NativeSession.acknowledgeStopped();
    }
    private void pollSession() throws Exception {
        String raw;while((raw=NativeSession.poll())!=null){JSONObject event=new JSONObject(raw);String type=event.getString("type");
            if(type.equals("load")){
                if(destroyed || !resumed){closeCore();NativeSession.stop();continue;}
                verifyCore();
                synchronized(NativeCore.class){NativeCore.close();NativeCore.configureBridge(BuildConfig.BRIDGE_ADDRESS,BuildConfig.SAVE_GENERATION_ADDRESS);if(!NativeCore.open(event.getString("rom"),event.getString("save")))throw new IOException("mGBA no pudo abrir la partida");connection=new BridgeConnection(event.getJSONObject("bridge"),event.getLong("epoch"));cooperative=true;}
                status.setText("Sesión adquirida · epoch "+event.getLong("epoch")+" · revisión "+event.getLong("revision")+(event.getBoolean("signature_verified")?" · firma pilot-v1 verificada":" · primer guardado pendiente")+". Presencia al llegar a Villa Raíz exterior.");
            }else if(type.equals("stop")){
                closeCore();
            }else if(type.equals("saved")){
                status.setText("Guardado aceptado por el servidor · revisión "+event.getLong("revision"));
            }else if(type.equals("reconnect_wait")){status.setText("Reconectando: esperando el período permitido por el servidor ("+((event.getLong("wait_ms")+999)/1000)+" s)…");}
            else if(type.equals("closed")){status.setText("Partida cerrada · revisión cloud "+event.getLong("revision"));}
            else if(type.equals("error")){closeCore();status.setText(event.getString("message"));}
        }
    }
    private void addKeys(LinearLayout parent,String[] labels,int[] masks){LinearLayout row=new LinearLayout(this);parent.addView(row);for(int i=0;i<labels.length;i++){Button b=new Button(this);b.setText(labels[i]);int mask=masks[i];b.setOnTouchListener((v,e)->{if(e.getActionMasked()==MotionEvent.ACTION_DOWN)game.keys|=mask;else if(e.getActionMasked()==MotionEvent.ACTION_UP || e.getActionMasked()==MotionEvent.ACTION_CANCEL)game.keys&=~mask;return true;});row.addView(b,new LinearLayout.LayoutParams(0,100,1));}}
    @Override protected void onActivityResult(int request,int result,Intent data){super.onActivityResult(request,result,data);if(request!=1 || result!=RESULT_OK || data==null)return;work(()->{
        if(pendingStart || NativeSession.isActive())throw new IOException("Hay una sesión activa");
        File temporary=new File(getFilesDir(),"import.tmp");
        try(InputStream in=getContentResolver().openInputStream(data.getData());OutputStream out=new FileOutputStream(temporary)){
            MessageDigest digest=MessageDigest.getInstance("SHA-256");byte[] b=new byte[65536];int n,total=0;
            while((n=in.read(b))!=-1){total+=n;if(total>32*1024*1024)throw new IOException("ROM demasiado grande");digest.update(b,0,n);out.write(b,0,n);}
            if(!MessageDigest.isEqual(digest.digest(),PinnedIdentity.hex(PinnedIdentity.ROM_HASH)))throw new SecurityException("La ROM no coincide con el build admitido");
        }catch(Exception e){temporary.delete();throw e;}
        closeCore();File rom=new File(getFilesDir(),"pokeemerald.gba");if(!temporary.renameTo(rom))throw new IOException("No se pudo importar la ROM");
        return "ROM verificada e importada. Introduce tu cuenta y pulsa Iniciar sesión y jugar.";
    });}
    @Override protected void onResume(){super.onResume();resumed=true;if(game!=null)game.start();}
    @Override protected void onPause(){resumed=false;if(game!=null){game.keys=0;if(NativeSession.isActive() || pendingStart){NativeSession.stop();}else game.stop();}super.onPause();}
    @Override protected void onDestroy(){destroyed=true;NativeSession.stop();worker.execute(()->{try{api.logout();}catch(Exception ignored){}});worker.shutdown();super.onDestroy();}
    private final class GameView extends View implements Runnable {
        volatile int keys;private Thread thread;private volatile boolean stop;
        private final Bitmap bitmap=Bitmap.createBitmap(240,160,Bitmap.Config.ARGB_8888);
        private final Paint paint=new Paint();
        GameView(){super(MainActivity.this);paint.setFilterBitmap(false);}
        void start(){if(thread!=null && thread.isAlive())return;stop=false;thread=new Thread(this,"gba-frame");thread.start();}
        void stop(){stop=true;if(thread!=null){try{thread.join(1000);}catch(InterruptedException e){Thread.currentThread().interrupt();}}}
        @Override public void run(){
            int size=Math.max(8192,AudioTrack.getMinBufferSize(32768,AudioFormat.CHANNEL_OUT_STEREO,AudioFormat.ENCODING_PCM_16BIT));
            AudioTrack audio=new AudioTrack(AudioManager.STREAM_MUSIC,32768,AudioFormat.CHANNEL_OUT_STEREO,AudioFormat.ENCODING_PCM_16BIT,size,AudioTrack.MODE_STREAM);audio.play();
            int[] pixels=new int[240*160];short[] samples=new short[4096];
            try{while(!stop && (resumed || pendingStart || NativeSession.isActive())){long started=System.nanoTime();int n;synchronized(NativeCore.class){n=NativeCore.frame(resumed?keys:0,pixels,samples);if(connection!=null)connection.step();}audio.setVolume(resumed?1:0);if(n>=0){synchronized(bitmap){bitmap.setPixels(pixels,0,240,0,0,240,160);}postInvalidate();if(n>0)audio.write(samples,0,n);}long left=16742706-(System.nanoTime()-started);if(left>0)TimeUnit.NANOSECONDS.sleep(left);}}
            catch(InterruptedException e){Thread.currentThread().interrupt();}catch(Exception e){NativeSession.stop();runOnUiThread(()->status.setText("Bridge detenido: "+e.getMessage()));}finally{audio.stop();audio.release();}
        }
        @Override protected void onDraw(Canvas canvas){super.onDraw(canvas);canvas.drawColor(Color.BLACK);int h=Math.min(getHeight(),getWidth()*2/3);synchronized(bitmap){canvas.drawBitmap(bitmap,null,new Rect(0,(getHeight()-h)/2,getWidth(),(getHeight()+h)/2),paint);}}
    }
}
