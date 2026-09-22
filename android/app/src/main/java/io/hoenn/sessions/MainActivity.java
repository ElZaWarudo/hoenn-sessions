package io.hoenn.sessions;

import android.app.Activity;
import android.app.AlertDialog;
import android.graphics.*;
import android.hardware.input.InputManager;
import android.media.*;
import android.os.*;
import android.text.InputType;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.*;

import java.util.concurrent.*;
import org.json.JSONObject;

public final class MainActivity extends Activity {
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final CloudApi api=new CloudApi();
    private TextView status;
    private LinearLayout layout, loginPanel, playPanel;
    private volatile boolean settingsOpen, inputFocused;
    private EditText user,password;
    private GameView game;
    private final ControllerInput controller=new ControllerInput();
    private InputManager inputManager;
    private final InputManager.InputDeviceListener controllerDevices=new InputManager.InputDeviceListener(){
        @Override public void onInputDeviceAdded(int deviceId){}
        @Override public void onInputDeviceChanged(int deviceId){controller.deviceRemoved(deviceId);}
        @Override public void onInputDeviceRemoved(int deviceId){controller.deviceRemoved(deviceId);}
    };
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
        ControllerSettingsDialog.loadPreferences(this,controller);
        inputManager=getSystemService(InputManager.class);
        inputManager.registerInputDeviceListener(controllerDevices,hostHandler);
        getWindow().setFlags(WindowManager.LayoutParams.FLAG_SECURE,WindowManager.LayoutParams.FLAG_SECURE);
        ScrollView scroll=new ScrollView(this);layout=new LinearLayout(this);layout.setOrientation(LinearLayout.VERTICAL);layout.setPadding(24,48,24,32);scroll.addView(layout);setContentView(scroll);
        TextView title=new TextView(this);title.setText("HOENN SESSIONS");title.setTextSize(23);layout.addView(title);
        button("Menú",this::showMenu);
        status=new TextView(this);status.setTag("session-status");status.setText("Inicia sesión para continuar tu partida.");status.setPadding(0,20,0,20);layout.addView(status);
        LinearLayout root=layout;
        loginPanel=new LinearLayout(this);loginPanel.setOrientation(LinearLayout.VERTICAL);root.addView(loginPanel);layout=loginPanel;
        user=field("Usuario",false);password=field("Contraseña",true);
        button("Iniciar sesión y jugar",()->{
            if(pendingStart || NativeSession.isActive()){status.setText("Ya hay una sesión activa o cerrándose.");return;}
            pendingStart=true;
            String u=user.getText().toString(),p=password.getText().toString();password.setText("");
            work(()->{try{
                if(destroyed || !resumed)return "Inicio cancelado al salir de la aplicación.";
                verifyCore();api.logout();closeCore();
                BundledRom.install(getFilesDir(),PinnedIdentity.ROM_HASH,()->getAssets().open("pokeemerald.gba"));
                copyManifest();
                if(!NativeSession.start(getFilesDir().getCanonicalPath(),u,p))throw new IOException("Ya hay una sesión activa");
                if(destroyed || !resumed)NativeSession.stop();
                return "Adquiriendo lease y verificando partida…";
            }finally{pendingStart=false;}});
        });
        layout=root;
        playPanel=new LinearLayout(this);playPanel.setOrientation(LinearLayout.VERTICAL);root.addView(playPanel);
        layout=playPanel;
        game=new GameView();game.setTag("gba-frame");layout.addView(game,new LinearLayout.LayoutParams(-1,-2));
        LinearLayout keys=new LinearLayout(this);keys.setOrientation(LinearLayout.VERTICAL);layout.addView(keys);
        addKeys(keys,new String[]{"↑","↓","←","→"},new int[]{64,128,32,16});
        addKeys(keys,new String[]{"A","B","L","R","START","SELECT"},new int[]{1,2,512,256,8,4});
        button("Cerrar partida (guarda antes en el juego)",()->{if(pendingStart || NativeSession.isActive()){status.setText("Cerrando y comprobando checkpoint…");NativeSession.stop();}else work(()->{closeCore();return "Juego local detenido.";});});
        layout=root;
        playPanel.setVisibility(View.GONE);
        hostHandler.post(hostPoll);
    }
    private void copyManifest() throws Exception {try(InputStream in=getAssets().open("bridge_manifest.json");OutputStream out=new FileOutputStream(new File(getFilesDir(),"bridge_manifest.json"))){byte[] b=new byte[8192];int n;while((n=in.read(b))!=-1)out.write(b,0,n);}}
    private void verifyCore() throws IOException {
        if(!"0.10.5|26b7884bc25a5933960f3cdcd98bac1ae14d42e2".equals(NativeCore.identity()))throw new IOException("Identidad del núcleo mGBA no admitida");
    }
    private void closeCore(){
        controller.clear();
        synchronized(NativeCore.class){try{if(connection!=null)connection.close();}finally{connection=null;NativeCore.close();cooperative=false;}}
        NativeSession.acknowledgeStopped();
    }
    private void pollSession() throws Exception {
        String raw;while((raw=NativeSession.poll())!=null){JSONObject event=new JSONObject(raw);String type=event.getString("type");
            if(type.equals("load")){
                if(destroyed || !resumed){closeCore();NativeSession.stop();continue;}
                verifyCore();
                synchronized(NativeCore.class){NativeCore.close();NativeCore.configureBridge(BuildConfig.BRIDGE_ADDRESS,BuildConfig.SAVE_GENERATION_ADDRESS);if(!NativeCore.open(event.getString("rom"),event.getString("save")))throw new IOException("mGBA no pudo abrir la partida");connection=new BridgeConnection(event.getJSONObject("bridge"),event.getLong("epoch"));cooperative=true;}
                loginPanel.setVisibility(View.GONE);playPanel.setVisibility(View.VISIBLE);
                status.setText("Sesión adquirida · epoch "+event.getLong("epoch")+" · revisión "+event.getLong("revision")+(event.getBoolean("signature_verified")?" · firma pilot-v1 verificada":" · primer guardado pendiente")+". Presencia al llegar a Villa Raíz exterior.");
            }else if(type.equals("stop")){
                closeCore();
            }else if(type.equals("saved")){
                status.setText("Guardado aceptado por el servidor · revisión "+event.getLong("revision"));
            }else if(type.equals("reconnect_wait")){status.setText("Reconectando: esperando el período permitido por el servidor ("+((event.getLong("wait_ms")+999)/1000)+" s)…");}
            else if(type.equals("closed")){loginPanel.setVisibility(View.VISIBLE);playPanel.setVisibility(View.GONE);status.setText("Partida cerrada · revisión cloud "+event.getLong("revision"));}
            else if(type.equals("error")){closeCore();loginPanel.setVisibility(View.VISIBLE);playPanel.setVisibility(View.GONE);status.setText(event.getString("message"));}
        }
    }
    private void addKeys(LinearLayout parent,String[] labels,int[] masks){LinearLayout row=new LinearLayout(this);parent.addView(row);for(int i=0;i<labels.length;i++){Button b=new Button(this);b.setText(labels[i]);int mask=masks[i];b.setOnTouchListener((v,e)->{if(e.getActionMasked()==MotionEvent.ACTION_DOWN)game.keys|=mask;else if(e.getActionMasked()==MotionEvent.ACTION_UP || e.getActionMasked()==MotionEvent.ACTION_CANCEL)game.keys&=~mask;return true;});row.addView(b,new LinearLayout.LayoutParams(0,100,1));}}
    @Override public boolean dispatchKeyEvent(KeyEvent event){
        if(cooperative && !settingsOpen && ControllerInput.isController(event) && (event.getAction()==KeyEvent.ACTION_DOWN || event.getAction()==KeyEvent.ACTION_UP)
                && controller.key(event.getDeviceId(),event.getKeyCode(),event.getAction()==KeyEvent.ACTION_DOWN))return true;
        return super.dispatchKeyEvent(event);
    }
    @Override public boolean dispatchGenericMotionEvent(MotionEvent event){
        if(cooperative && !settingsOpen && event.isFromSource(InputDevice.SOURCE_JOYSTICK) && event.getActionMasked()==MotionEvent.ACTION_MOVE){
            controller.axes(event.getDeviceId(),event.getAxisValue(MotionEvent.AXIS_X),event.getAxisValue(MotionEvent.AXIS_Y),
                    event.getAxisValue(MotionEvent.AXIS_HAT_X),event.getAxisValue(MotionEvent.AXIS_HAT_Y));
            return true;
        }
        return super.dispatchGenericMotionEvent(event);
    }
    private void showMenu(){
        new AlertDialog.Builder(this).setTitle("Menú")
            .setItems(new String[]{"Configurar mando","Reconectar desde último guardado cloud","Cerrar sesión","Registrar cuenta","Comprobar conexión"},(dialog,item)->{
                if(item==0){
                    settingsOpen=true;controller.clear();game.keys=0;
                    new ControllerSettingsDialog(this,controller,()->{controller.clear();settingsOpen=false;}).show();
                }else if(item==1)reconnectSession();
                else if(item==2)stopSession();
                else if(item==3)registerAccount();
                else work(()->{api.health();return "Conexión segura · servidor disponible.";});
            }).show();
    }
    void reconnectSession(){status.setText(NativeSession.reconnect()?"Reconectando desde tu último guardado…":"Necesitas una sesión activa y un guardado aceptado por el servidor.");}
    void stopSession(){NativeSession.stop();status.setText("Cerrando y comprobando el guardado…");}
    private void registerAccount(){
        LinearLayout form=new LinearLayout(this);form.setOrientation(LinearLayout.VERTICAL);
        EditText name=new EditText(this);name.setHint("Usuario");form.addView(name);
        EditText secret=new EditText(this);secret.setHint("Contraseña");secret.setInputType(129);secret.setSaveEnabled(false);form.addView(secret);
        EditText code=new EditText(this);code.setHint("Invitación");code.setInputType(129);code.setSaveEnabled(false);form.addView(code);
        new AlertDialog.Builder(this).setTitle("Registrar cuenta").setView(form).setNegativeButton("Cancelar",null)
            .setPositiveButton("Registrar",(d,w)->{String u=name.getText().toString(),p=secret.getText().toString(),i=code.getText().toString();secret.setText("");code.setText("");work(()->{api.register(u,p,i);return "Cuenta registrada. Ya puedes iniciar sesión.";});}).show();
    }
    @Override protected void onResume(){super.onResume();resumed=true;if(game!=null)game.start();}
    @Override public void onWindowFocusChanged(boolean hasFocus){super.onWindowFocusChanged(hasFocus);inputFocused=hasFocus;if(!hasFocus){controller.clear();if(game!=null)game.keys=0;}}
    @Override protected void onPause(){resumed=false;controller.clear();if(game!=null){game.keys=0;if(NativeSession.isActive() || pendingStart){NativeSession.stop();}else game.stop();}super.onPause();}
    @Override protected void onDestroy(){destroyed=true;inputManager.unregisterInputDeviceListener(controllerDevices);NativeSession.stop();worker.execute(()->{try{api.logout();}catch(Exception ignored){}});worker.shutdown();super.onDestroy();}
    private final class GameView extends View implements Runnable {
        volatile int keys;private Thread thread;private volatile boolean stop;
        private final Bitmap bitmap=Bitmap.createBitmap(240,160,Bitmap.Config.ARGB_8888);
        private final Paint paint=new Paint();
        GameView(){super(MainActivity.this);paint.setFilterBitmap(false);}
        void start(){if(thread!=null && thread.isAlive())return;stop=false;thread=new Thread(this,"gba-frame");thread.start();}
        void stop(){stop=true;if(thread!=null){try{thread.join(1000);}catch(InterruptedException e){Thread.currentThread().interrupt();}}}
        @Override public void run(){
            int size=Math.max(8192,AudioTrack.getMinBufferSize(32768,AudioFormat.CHANNEL_OUT_STEREO,AudioFormat.ENCODING_PCM_16BIT));
            AudioTrack audio=null;
            try{
                audio=new AudioTrack(AudioManager.STREAM_MUSIC,32768,AudioFormat.CHANNEL_OUT_STEREO,AudioFormat.ENCODING_PCM_16BIT,size,AudioTrack.MODE_STREAM);
                if(audio.getState()!=AudioTrack.STATE_INITIALIZED)throw new IllegalStateException("Audio unavailable");
                audio.play();
            }catch(IllegalArgumentException | IllegalStateException e){if(audio!=null)audio.release();audio=null;}
            int[] pixels=new int[240*160];short[] samples=new short[4096];
            try{while(!stop && (resumed || pendingStart || NativeSession.isActive())){long started=System.nanoTime();int n;synchronized(NativeCore.class){n=NativeCore.frame(resumed && inputFocused && !settingsOpen?(keys | controller.keys()):0,pixels,samples);if(connection!=null)connection.step();}if(audio!=null)audio.setVolume(resumed?1:0);if(n>=0){synchronized(bitmap){bitmap.setPixels(pixels,0,240,0,0,240,160);}postInvalidate();if(n>0 && audio!=null)audio.write(samples,0,n);}long left=16742706-(System.nanoTime()-started);if(left>0)TimeUnit.NANOSECONDS.sleep(left);}}
            catch(InterruptedException e){Thread.currentThread().interrupt();}catch(Exception e){NativeSession.stop();runOnUiThread(()->status.setText("Bridge detenido: "+e.getMessage()));}finally{if(audio!=null){audio.stop();audio.release();}}
        }
        @Override protected void onMeasure(int widthSpec,int heightSpec){
            int width=MeasureSpec.getSize(widthSpec);
            setMeasuredDimension(width,resolveSize(width*2/3,heightSpec));
        }
        @Override protected void onDraw(Canvas canvas){
            super.onDraw(canvas);canvas.drawColor(Color.BLACK);
            float scale=Math.min(getWidth()/240f,getHeight()/160f);
            int w=Math.round(240*scale),h=Math.round(160*scale);
            Rect destination=new Rect((getWidth()-w)/2,(getHeight()-h)/2,(getWidth()+w)/2,(getHeight()+h)/2);
            synchronized(bitmap){canvas.drawBitmap(bitmap,null,destination,paint);}
        }
    }
}
