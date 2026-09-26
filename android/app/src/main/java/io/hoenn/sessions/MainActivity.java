package io.hoenn.sessions;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.SharedPreferences;
import android.graphics.*;
import android.hardware.input.InputManager;
import android.media.*;
import android.os.*;
import android.text.InputType;
import android.util.Log;
import android.view.*;
import android.widget.*;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;
import java.io.*;
import java.nio.*;

import java.util.concurrent.*;
import org.json.JSONObject;

public final class MainActivity extends Activity implements GameRenderer.Host {
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final CloudApi api=new CloudApi();
    private RuntimeStore.Game currentGame;
    private TextView status;
    private ProgressBar downloadProgress;
    private TextView downloadLabel;
    private TextView gameStatus, connectionStatus, cloudSaveStatus, debugStatus;
    private AlertDialog unsavedDialog;
    private volatile boolean integerScale, smoothPixels, showStats;
    private volatile long pingMs=-1;
    private long nextPingAt;
    private final Runnable hideGameStatus=()->{if(gameStatus!=null)gameStatus.animate().alpha(0f).setDuration(400).start();};
    private LinearLayout layout, loginPanel;
    private FrameLayout playPanel;
    private View loginScreen;
    private volatile boolean settingsOpen, inputFocused;
    private EditText user,password;
    private GameRenderer game;
    private TouchOverlay touchOverlay;
    private OnBackInvokedCallback backCallback;
    private SecureCredentialStore.Account savedAccount;
    private final ControllerInput controller=new ControllerInput();
    private boolean menuHotkeyDown;
    private InputManager inputManager;
    private final InputManager.InputDeviceListener controllerDevices=new InputManager.InputDeviceListener(){
        @Override public void onInputDeviceAdded(int deviceId){refreshControllerOverlay();}
        @Override public void onInputDeviceChanged(int deviceId){controller.deviceRemoved(deviceId);refreshControllerOverlay();}
        @Override public void onInputDeviceRemoved(int deviceId){controller.deviceRemoved(deviceId);refreshControllerOverlay();}
    };
    private void refreshControllerOverlay(){
        if(touchOverlay==null)return;
        boolean present=false;
        for(int id:InputDevice.getDeviceIds()){
            InputDevice device=InputDevice.getDevice(id);
            if(device!=null && ((device.getSources()&InputDevice.SOURCE_GAMEPAD)==InputDevice.SOURCE_GAMEPAD
                || (device.getSources()&InputDevice.SOURCE_JOYSTICK)==InputDevice.SOURCE_JOYSTICK)){present=true;break;}
        }
        touchOverlay.setControllerPresent(present);
    }
    private volatile BridgeConnection connection;
    private volatile boolean cooperative;
    private long lastSeenSaveSerial,lastAcceptedSaveSerial,lastCloudRevision;
    private boolean savePending;
    private long nextEvidenceAt;
    private final SessionController session=new SessionController();
    private final Handler hostHandler=new Handler(Looper.getMainLooper());
    private final Runnable hostPoll=new Runnable(){public void run(){
        try{
            pollSession();pollSaveEvidence();pollPing();
            if(session.isResumed() && session.takeAutoResumeIfReady(NativeSession.isActive()||session.isStarting()))startSavedSession();
        }catch(Exception e){Log.e("HoennGame","Session polling failed",e);showStatus(getString(R.string.operation_failed));NativeSession.stop();closeCore();session.markReady();}
        if(session.isDestroyed() && !session.isStarting() && !NativeSession.isActive()){game.stop();closeCore();return;}
        hostHandler.postDelayed(this,session.isResumed()?50:250);
    }};
    private volatile boolean audioFocused;
    private AudioManager audioManager;
    private AudioFocusRequest audioFocusRequest;
    private final AudioManager.OnAudioFocusChangeListener audioFocusChange=change->{
        audioFocused=change==AudioManager.AUDIOFOCUS_GAIN;
    };
    private final BroadcastReceiver noisyAudio=new BroadcastReceiver(){
        @Override public void onReceive(Context context,Intent intent){
            if(AudioManager.ACTION_AUDIO_BECOMING_NOISY.equals(intent.getAction()) && cooperative){
                audioFocused=false;
                showStatus(getString(R.string.audio_disconnected));
                new AlertDialog.Builder(MainActivity.this).setTitle(R.string.game_paused)
                    .setMessage(R.string.headphones_disconnected)
                    .setPositiveButton(R.string.resume,(dialog,which)->audioFocused=session.isResumed())
                    .show();
            }
        }
    };
    private boolean awaitingInstallPermission;
    private int dp(int value){return Math.round(value*getResources().getDisplayMetrics().density);}
    private void showStatus(String message){
        if(Looper.myLooper()!=Looper.getMainLooper()){runOnUiThread(()->showStatus(message));return;}
        if(status!=null)status.setText(message);
        if(gameStatus!=null){
            gameStatus.setText(message);
            gameStatus.animate().cancel();gameStatus.setAlpha(1f);
            hostHandler.removeCallbacks(hideGameStatus);hostHandler.postDelayed(hideGameStatus,5000);
        }
    }
    private void setConnection(String label,int color){
        if(Looper.myLooper()!=Looper.getMainLooper()){runOnUiThread(()->setConnection(label,color));return;}
        if(connectionStatus!=null){connectionStatus.setText(label);connectionStatus.setTextColor(color);}
    }
    private void setCloudRevision(long revision){
        if(cloudSaveStatus!=null)cloudSaveStatus.setText(revision>0?getString(R.string.cloud_save_revision,revision):getString(R.string.cloud_save_pending));
    }
    private void onDownloadProgress(long received,long total){
        runOnUiThread(()->{
            if(session.isDestroyed() || downloadProgress==null)return;
            downloadProgress.setVisibility(View.VISIBLE);downloadLabel.setVisibility(View.VISIBLE);
            downloadProgress.setMax(1000);downloadProgress.setProgress(total>0?(int)Math.min(1000,received*1000/total):0);
            downloadLabel.setText(getString(R.string.download_progress,received/(1024*1024),(total+1024*1024-1)/(1024*1024)));
        });
    }
    private void hideDownloadProgress(){
        runOnUiThread(()->{if(downloadProgress!=null){downloadProgress.setVisibility(View.GONE);downloadLabel.setVisibility(View.GONE);}});
    }
    private void setPlaying(boolean playing){
        loginScreen.setVisibility(playing?View.GONE:View.VISIBLE);
        playPanel.setVisibility(playing?View.VISIBLE:View.GONE);
        if(playing)getWindow().clearFlags(WindowManager.LayoutParams.FLAG_SECURE);
        else getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
    }
    private void pollPing(){
        if(!showStats || !cooperative || session.isDestroyed())return;
        long now=SystemClock.elapsedRealtime();if(now<nextPingAt)return;
        nextPingAt=now+10000;
        worker.execute(()->{
            long started=SystemClock.elapsedRealtime();
            try{api.health();pingMs=SystemClock.elapsedRealtime()-started;}
            catch(Exception ignored){pingMs=-1;}
        });
    }
    private void pollSaveEvidence(){
        if(!cooperative || session.isDestroyed() || SystemClock.elapsedRealtime()<nextEvidenceAt)return;
        nextEvidenceAt=SystemClock.elapsedRealtime()+500;
        long[] evidence=NativeCore.saveEvidence();
        if(evidence!=null && evidence.length>0){
            lastSeenSaveSerial=evidence[0];
            savePending=lastSeenSaveSerial>lastAcceptedSaveSerial;
        }
    }
    private interface Work {String run() throws Exception;}
    private String describeError(Exception error){
        if(error instanceof CloudApi.HttpError)return getString(R.string.server_rejected,((CloudApi.HttpError)error).status);
        if(error instanceof SecurityException)return getString(R.string.server_verification_failed);
        if(error instanceof IOException){
            String message=error.getMessage();
            return error.getClass()==IOException.class && message!=null && !message.contains("Exception")
                ? message:getString(R.string.server_unreachable);
        }
        Log.e("HoennGame","Operación fallida",error);
        return getString(R.string.operation_failed);
    }
    private void work(Work work) {
        worker.execute(()->{try{String result=work.run(); runOnUiThread(()->showStatus(result));}catch(Exception e){runOnUiThread(()->showStatus(describeError(e)));}});
    }
    private void button(String label,Runnable action){Button b=new Button(this);b.setText(label);b.setOnClickListener(v->action.run());layout.addView(b);}
    private EditText field(String label,boolean secret){EditText e=new EditText(this);e.setHint(label);e.setSingleLine();e.setSaveEnabled(false);e.setInputType(secret?129:InputType.TYPE_CLASS_TEXT);layout.addView(e);return e;}
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);
        ApkUpdate.cleanupInstalled(this);
        audioManager=getSystemService(AudioManager.class);
        registerReceiver(noisyAudio,new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY));
        AudioAttributes audioAttributes=new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build();
        audioFocusRequest=new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(audioAttributes).setOnAudioFocusChangeListener(audioFocusChange).build();
        SecureCredentialStore.initialize(this);
        SharedPreferences display=getSharedPreferences("display_options",MODE_PRIVATE);
        integerScale=display.getBoolean("integer_scale",false);smoothPixels=display.getBoolean("smooth_pixels",false);showStats=display.getBoolean("show_stats",false);
        savedAccount=SecureCredentialStore.loadAccount();
        session.setAutoResumePending(savedAccount!=null);
        ControllerSettingsDialog.loadPreferences(this,controller);
        inputManager=getSystemService(InputManager.class);
        inputManager.registerInputDeviceListener(controllerDevices,hostHandler);
        getWindow().setFlags(WindowManager.LayoutParams.FLAG_SECURE,WindowManager.LayoutParams.FLAG_SECURE);
        if(Build.VERSION.SDK_INT>=28){WindowManager.LayoutParams params=getWindow().getAttributes();params.layoutInDisplayCutoutMode=WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_NEVER;getWindow().setAttributes(params);}
        FrameLayout root=new FrameLayout(this);setContentView(root);
        ScrollView scroll=new ScrollView(this);loginScreen=scroll;layout=new LinearLayout(this);layout.setOrientation(LinearLayout.VERTICAL);layout.setPadding(dp(24),dp(48),dp(24),dp(32));scroll.addView(layout);root.addView(scroll,new FrameLayout.LayoutParams(-1,-1));
        TextView title=new TextView(this);title.setText("HOENN SESSIONS");title.setTextSize(23);layout.addView(title);
        button(getString(R.string.menu),this::showMenu);
        status=new TextView(this);status.setTag("session-status");showStatus(getString(R.string.login_prompt));status.setPadding(0,dp(20),0,dp(20));layout.addView(status);
        downloadProgress=new ProgressBar(this,null,android.R.attr.progressBarStyleHorizontal);downloadProgress.setVisibility(View.GONE);layout.addView(downloadProgress,new LinearLayout.LayoutParams(-1,dp(6)));
        downloadLabel=new TextView(this);downloadLabel.setVisibility(View.GONE);layout.addView(downloadLabel);
        LinearLayout loginRoot=layout;
        loginPanel=new LinearLayout(this);loginPanel.setOrientation(LinearLayout.VERTICAL);loginRoot.addView(loginPanel);layout=loginPanel;
        user=field(getString(R.string.username),false);password=field(getString(R.string.password),true);
        button(getString(R.string.login_play),this::startWithPassword);
        button(getString(R.string.register_account),this::registerAccount);
        button(getString(R.string.resume_saved_session),()->{if(SecureCredentialStore.loadAccount()==null)showStatus(getString(R.string.no_saved_session));else{session.resetRetries();startSavedSession();}});
        layout=loginRoot;
        playPanel=new FrameLayout(this);root.addView(playPanel,new FrameLayout.LayoutParams(-1,-1));
        touchOverlay=new TouchOverlay(this);touchOverlay.setTag("touch-overlay");playPanel.addView(touchOverlay,new FrameLayout.LayoutParams(-1,-1));
        game=new GameRenderer(this,this,touchOverlay,controller,smoothPixels);game.setTag("gba-frame");playPanel.addView(game,0,new FrameLayout.LayoutParams(-1,-1));
        refreshControllerOverlay();
        LinearLayout hud=new LinearLayout(this);hud.setOrientation(LinearLayout.VERTICAL);hud.setPadding(dp(8),dp(8),dp(8),dp(8));
        FrameLayout.LayoutParams hudPosition=new FrameLayout.LayoutParams(-2,-2,Gravity.TOP|Gravity.START);hudPosition.setMargins(dp(12),dp(12),dp(12),0);playPanel.addView(hud,hudPosition);
        connectionStatus=new TextView(this);connectionStatus.setBackgroundColor(0xB0000000);connectionStatus.setPadding(dp(8),dp(3),dp(8),dp(3));hud.addView(connectionStatus);setConnection(getString(R.string.connection_offline),0xFFFF8A80);
        cloudSaveStatus=new TextView(this);cloudSaveStatus.setTextColor(Color.WHITE);cloudSaveStatus.setBackgroundColor(0xB0000000);cloudSaveStatus.setPadding(dp(8),dp(3),dp(8),dp(3));hud.addView(cloudSaveStatus);setCloudRevision(0);
        debugStatus=new TextView(this);debugStatus.setTextColor(Color.WHITE);debugStatus.setBackgroundColor(0xB0000000);debugStatus.setPadding(dp(8),dp(3),dp(8),dp(3));debugStatus.setVisibility(showStats?View.VISIBLE:View.GONE);hud.addView(debugStatus);
        gameStatus=new TextView(this);gameStatus.setTag("game-status-chip");gameStatus.setTextColor(Color.WHITE);gameStatus.setBackgroundColor(0xD0000000);gameStatus.setPadding(dp(8),dp(5),dp(8),dp(5));gameStatus.setMaxWidth(dp(360));gameStatus.setAlpha(0f);hud.addView(gameStatus);
        playPanel.setVisibility(View.GONE);
        String installResult=ApkUpdate.handleInstallResult(this,getIntent());if(installResult!=null)showStatus(installResult);
        if(Build.VERSION.SDK_INT>=33){backCallback=this::handleBack;getOnBackInvokedDispatcher().registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT,backCallback);}
        hostHandler.post(hostPoll);
        if(savedAccount!=null)showStatus(getString(R.string.restoring_account,savedAccount.username));
    }

    private void startWithPassword(){
        session.resetRetries();hostHandler.removeCallbacks(resumeRetry);
        String username=user.getText().toString().trim(),secret=password.getText().toString();password.setText("");
        if(username.isEmpty() || secret.isEmpty()){showStatus(getString(R.string.enter_credentials));return;}
        startSession(username,secret,"","",false);
    }

    private void startSavedSession(){
        savedAccount=SecureCredentialStore.loadAccount();
        if(savedAccount==null)return;
        startSession(savedAccount.username,"",savedAccount.userId,savedAccount.characterId,true);
    }

    private void startSession(String username,String secret,String userId,String characterId,boolean resumeSession){
        if(NativeSession.isActive() || !session.beginStart()){showStatus(getString(R.string.session_active));return;}
        work(()->{try{
            if(session.isDestroyed() || !session.isResumed())return getString(R.string.start_cancelled);
            verifyCore();
            new RuntimeStore(getFilesDir()).prepareBundled(getAssets());
            api.authenticate(username,secret);
            ApkUpdate.Available update=ApkUpdate.check(api);
            if(update!=null) {
                session.updatePromptShown();
                runOnUiThread(()->promptApkUpdate(update,username,userId,characterId,resumeSession));
                return getString(R.string.update_available_status);
            }
            return beginGame(username,userId,characterId,resumeSession);
        }catch(IOException error){
            if(resumeSession && session.isResumed() && !session.isDestroyed() && session.retryCount()<15){
                return scheduleResumeRetry(getString(R.string.connection_interrupted));
            }
            throw error;
        }finally{session.finishStart();}});
    }
    private String beginGame(String username,String userId,String characterId,boolean resumeSession) throws Exception {
        closeCore();
        try{currentGame=new RuntimeStore(getFilesDir()).ensureLatest(api,this::onDownloadProgress);}
        finally{hideDownloadProgress();}
        String authenticatedUserId=resumeSession?userId:api.userId();
        String authenticatedCharacterId=resumeSession?characterId:api.characterId();
        api.clearAccess();
        // Java's preflight login created the refresh family; Rust rotates that same family.
        if(!NativeSession.start(getFilesDir().getCanonicalPath(),username,"",authenticatedUserId,authenticatedCharacterId,true,false))throw new IOException(getString(R.string.game_session_active));
        if(session.isDestroyed() || !session.isResumed())NativeSession.stop();
        return resumeSession?getString(R.string.restoring_session):getString(R.string.checking_credentials);
    }
    private void promptApkUpdate(ApkUpdate.Available update,String username,String userId,String characterId,boolean resumeSession) {
        if(session.isDestroyed()){session.updatePromptFinished();return;}
        new AlertDialog.Builder(this).setTitle(R.string.update_available)
            .setMessage(R.string.update_message)
            .setPositiveButton(R.string.update,(dialog,which)->worker.execute(()->{
                try {
                    try{ApkUpdate.download(api,this,update,this::onDownloadProgress);}
                    finally{hideDownloadProgress();}
                    runOnUiThread(()->{openApkInstaller();session.updatePromptFinished();});
                } catch(Exception error) {
                    runOnUiThread(()->{showStatus(describeError(error));session.updatePromptFinished();});
                }
            }))
            .setNegativeButton(R.string.later,(dialog,which)->work(()->{
                try {return beginGame(username,userId,characterId,resumeSession);}
                finally {session.updatePromptFinished();}
            }))
            .setOnCancelListener(dialog->session.updatePromptFinished())
            .show();
    }
    private void verifyCore() throws IOException {
        if(!"0.10.5|26b7884bc25a5933960f3cdcd98bac1ae14d42e2".equals(NativeCore.identity()))throw new IOException(getString(R.string.core_identity_invalid));
    }
    private void openApkInstaller() {
        try { awaitingInstallPermission=!ApkUpdate.openInstaller(this); }
        catch (RuntimeException error) { awaitingInstallPermission=false;showStatus(getString(R.string.installer_open_failed,error.getMessage())); }
    }
    private void closeCore(){
        controller.clear();
        synchronized(NativeCore.class){try{if(connection!=null)connection.close();}finally{connection=null;NativeCore.close();cooperative=false;}}
        NativeSession.acknowledgeStopped();
    }
    private void pollSession() throws Exception {
        String raw;while((raw=NativeSession.poll())!=null){JSONObject event=new JSONObject(raw);String type=event.getString("type");
            if(type.equals("load")){
                session.resetRetries();hostHandler.removeCallbacks(resumeRetry);
                if(!session.mayAcceptLoad(currentGame!=null)){
                    closeCore();NativeSession.stop();continue;
                }
                verifyCore();
                synchronized(NativeCore.class){NativeCore.close();NativeBridge.ADDRESS=currentGame.bridgeAddress();NativeCore.configureBridge(NativeBridge.ADDRESS,currentGame.generationAddress());if(!NativeCore.open(event.getString("rom"),event.getString("save")))throw new IOException(getString(R.string.game_open_failed));connection=new BridgeConnection(event.getJSONObject("bridge"),event.getLong("epoch"));cooperative=true;}
                savedAccount=SecureCredentialStore.loadAccount();session.markPlaying();setPlaying(true);enterFullscreen();
                lastCloudRevision=event.getLong("revision");
                long[] evidence=NativeCore.saveEvidence();lastSeenSaveSerial=evidence!=null&&evidence.length>0?evidence[0]:0;lastAcceptedSaveSerial=lastSeenSaveSerial;savePending=false;
                setConnection(getString(R.string.connection_online),0xFF80CBC4);setCloudRevision(lastCloudRevision);
                showStatus(getString(R.string.session_acquired,event.getLong("epoch"),event.getLong("revision"),event.getBoolean("signature_verified")?getString(R.string.signature_verified):getString(R.string.first_save_pending)));
            }else if(type.equals("stop")){
                closeCore();session.markReady();
            }else if(type.equals("saved")){
                lastCloudRevision=event.getLong("revision");
                Long accepted=connection!=null?connection.pollSubmittedSaveSerial():null;
                if(accepted!=null)lastAcceptedSaveSerial=Math.max(lastAcceptedSaveSerial,accepted);
                long[] evidence=NativeCore.saveEvidence();
                if(evidence!=null && evidence.length>0)lastSeenSaveSerial=evidence[0];
                savePending=lastSeenSaveSerial>lastAcceptedSaveSerial;
                setConnection(getString(R.string.connection_online),0xFF80CBC4);setCloudRevision(lastCloudRevision);
                showStatus(getString(R.string.save_accepted,event.getLong("revision")));
            }else if(type.equals("reconnect_wait")){setConnection(getString(R.string.connection_reconnecting),0xFFFFD180);showStatus(getString(R.string.reconnect_wait,(event.getLong("wait_ms")+999)/1000));}
            else if(type.equals("closed")){
                exitFullscreen();setPlaying(false);loginPanel.setVisibility(View.VISIBLE);setConnection(getString(R.string.connection_offline),0xFFFF8A80);
                savePending=false;
                boolean signedOut=event.optBoolean("signed_out",false);
                session.markReady();
                if(signedOut){SecureCredentialStore.clearLocalAccount();savedAccount=null;showStatus(getString(R.string.signed_out));}
                else {savedAccount=SecureCredentialStore.loadAccount();showStatus(getString(R.string.game_closed_revision,event.getLong("revision")));}
                if(session.wantsResumeAfterPause() && session.isResumed() && !signedOut)resumeAfterClose();
            }else if(type.equals("error")){
                closeCore();session.markReady();savedAccount=SecureCredentialStore.loadAccount();exitFullscreen();
                setPlaying(false);loginPanel.setVisibility(View.VISIBLE);setConnection(getString(R.string.connection_offline),0xFFFF8A80);
                String message=event.getString("message");
                String code=event.optString("code","");
                boolean leaseConflict=code.equals("lease_conflict");
                boolean cloudFailure=code.equals("cloud_unreachable");
                if(session.isResumed()&&savedAccount!=null&&(leaseConflict||cloudFailure)&&session.retryCount()<15){
                    showStatus(scheduleResumeRetry(leaseConflict?getString(R.string.other_session):getString(R.string.connection_interrupted)));
                }else if(leaseConflict)showStatus(getString(R.string.other_session_retry));
                else if(cloudFailure)showStatus(getString(R.string.server_retry));
                else {Log.e("HoennGame","Session error: "+message);showStatus(getString(R.string.operation_failed));}
            }
        }
    }
    @Override public boolean dispatchKeyEvent(KeyEvent event){
        if(cooperative && !settingsOpen && ControllerInput.isController(event) && (event.getAction()==KeyEvent.ACTION_DOWN || event.getAction()==KeyEvent.ACTION_UP)){
            boolean down=event.getAction()==KeyEvent.ACTION_DOWN;
            if(controller.key(event.getDeviceId(),event.getKeyCode(),down)){
                boolean menu=controller.menuPressed() || (controller.keys()&12)==12;
                if(menu && !menuHotkeyDown && down && event.getRepeatCount()==0){
                    menuHotkeyDown=true;controller.clear();showMenu();
                }else if(!menu && !down)menuHotkeyDown=false;
                return true;
            }
        }
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
        new AlertDialog.Builder(this).setTitle(R.string.menu)
            .setItems(new String[]{getString(R.string.menu_touch_controls),getString(R.string.menu_controller),getString(R.string.menu_reconnect),getString(R.string.menu_close_game),getString(R.string.menu_sign_out),getString(R.string.register_account),getString(R.string.menu_check_connection),getString(R.string.menu_install_update),getString(R.string.menu_display_options)},(dialog,item)->{
                if(item==0)configureTouchOverlay();
                else if(item==1){
                    settingsOpen=true;controller.clear();game.keys=0;
                    new ControllerSettingsDialog(this,controller,()->{controller.clear();settingsOpen=false;}).show();
                }else if(item==2)reconnectSession();
                else if(item==3)stopSession();
                else if(item==4)signOut();
                else if(item==5)registerAccount();
                else if(item==6)work(()->{api.health();return getString(R.string.connection_available);});
                else if(item==8)showDisplayOptions();
                else if(new File(new File(getFilesDir(),"updates"),"update.apk").isFile())openApkInstaller();
                else showStatus(getString(R.string.no_downloaded_update));
            }).setOnDismissListener(dialog->menuHotkeyDown=false).show();
    }

    private void showDisplayOptions(){
        LinearLayout options=new LinearLayout(this);options.setOrientation(LinearLayout.VERTICAL);options.setPadding(dp(16),dp(8),dp(16),dp(8));
        CheckBox integer=new CheckBox(this);integer.setText(R.string.integer_scale);integer.setChecked(integerScale);options.addView(integer);
        CheckBox smooth=new CheckBox(this);smooth.setText(R.string.smooth_pixels);smooth.setChecked(smoothPixels);options.addView(smooth);
        CheckBox stats=new CheckBox(this);stats.setText(R.string.show_stats);stats.setChecked(showStats);options.addView(stats);
        new AlertDialog.Builder(this).setTitle(R.string.display).setView(options)
            .setPositiveButton(R.string.save,(dialog,which)->{
                integerScale=integer.isChecked();smoothPixels=smooth.isChecked();showStats=stats.isChecked();
                getSharedPreferences("display_options",MODE_PRIVATE).edit()
                    .putBoolean("integer_scale",integerScale).putBoolean("smooth_pixels",smoothPixels).putBoolean("show_stats",showStats).apply();
                game.setSmoothPixels(smoothPixels);debugStatus.setVisibility(showStats?View.VISIBLE:View.GONE);game.invalidate();
            }).setNegativeButton(R.string.cancel,null).show();
    }

    private void enterFullscreen(){
        if(Build.VERSION.SDK_INT>=30){
            getWindow().setDecorFitsSystemWindows(false);
            WindowInsetsController insets=getWindow().getInsetsController();
            if(insets!=null){
                insets.hide(WindowInsets.Type.statusBars()|WindowInsets.Type.navigationBars());
                insets.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        }else{
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY|View.SYSTEM_UI_FLAG_FULLSCREEN|View.SYSTEM_UI_FLAG_HIDE_NAVIGATION|View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN|View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION|View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
    }

    private void exitFullscreen(){
        if(Build.VERSION.SDK_INT>=30){
            getWindow().setDecorFitsSystemWindows(true);
            WindowInsetsController insets=getWindow().getInsetsController();
            if(insets!=null)insets.show(WindowInsets.Type.statusBars()|WindowInsets.Type.navigationBars());
        }else getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_VISIBLE);
    }
    private void configureTouchOverlay(){
        if(touchOverlay.isEditing()){
            touchOverlay.setEditing(false);showStatus(getString(R.string.controls_position_saved));return;
        }
        settingsOpen=true;controller.clear();game.keys=0;
        TouchOverlaySettingsDialog.show(this,touchOverlay,()->{
            touchOverlay.setEditing(true);settingsOpen=false;
            Toast.makeText(this,R.string.controls_edit_hint,Toast.LENGTH_LONG).show();
        },()->settingsOpen=false);
    }
    private void confirmUnsavedExit(Runnable action){
        if(!cooperative || (!savePending && lastCloudRevision>0)){action.run();return;}
        unsavedDialog=new AlertDialog.Builder(this).setTitle(R.string.cloud_save_pending)
            .setMessage(R.string.unsaved_message)
            .setNegativeButton(R.string.keep_playing,null)
            .setPositiveButton(R.string.continue_anyway,(dialog,which)->action.run()).create();
        unsavedDialog.setOnDismissListener(dialog->unsavedDialog=null);
        unsavedDialog.show();
    }
    AlertDialog pendingUnsavedDialog(){return unsavedDialog;}
    void reconnectSession(){confirmUnsavedExit(()->{
        boolean accepted=NativeSession.reconnect();
        if(accepted)session.markReconnecting();
        showStatus(accepted?getString(R.string.reconnect_from_save):getString(R.string.reconnect_unavailable));
    });}
    void stopSession(){confirmUnsavedExit(()->{session.markClosing();NativeSession.stop();showStatus(getString(R.string.closing_and_saving));});}
    private void signOut(){
        if(cooperative){confirmUnsavedExit(this::signOutUnchecked);return;}
        signOutUnchecked();
    }
    private void signOutUnchecked(){
        session.clearResumeAfterPause();
        if(NativeSession.isActive()){session.markClosing();NativeSession.signOut();showStatus(getString(R.string.closing_session));return;}
        savedAccount=SecureCredentialStore.loadAccount();
        if(savedAccount==null){showStatus(getString(R.string.no_active_session));return;}
        if(session.isStarting()){showStatus(getString(R.string.wait_for_login));return;}
        if(!session.beginStart()){showStatus(getString(R.string.wait_for_login));return;}
        SecureCredentialStore.Account account=savedAccount;
        work(()->{try{
            if(!NativeSession.start(getFilesDir().getCanonicalPath(),account.username,"",account.userId,account.characterId,true,true))throw new IOException(getString(R.string.other_operation));
            return getString(R.string.signing_out);
        }finally{session.finishStart();}});
    }
    private void registerAccount(){
        LinearLayout form=new LinearLayout(this);form.setOrientation(LinearLayout.VERTICAL);
        EditText name=new EditText(this);name.setHint(R.string.username);form.addView(name);
        EditText secret=new EditText(this);secret.setHint(R.string.password);secret.setInputType(129);secret.setSaveEnabled(false);form.addView(secret);
        EditText code=new EditText(this);code.setHint(R.string.invitation);code.setInputType(129);code.setSaveEnabled(false);form.addView(code);
        new AlertDialog.Builder(this).setTitle(R.string.register_account).setView(form).setNegativeButton(R.string.cancel,null)
            .setPositiveButton(R.string.register,(d,w)->{String u=name.getText().toString(),p=secret.getText().toString(),i=code.getText().toString();secret.setText("");code.setText("");work(()->{api.register(u,p,i);return getString(R.string.account_registered);});}).show();
    }
    private void resumeAfterClose(){
        if(!session.wantsResumeAfterPause() || session.isDestroyed())return;
        if(!session.isResumed())return;
        if(session.retryCount()>0)return;
        if(NativeSession.isActive() || session.isStarting()){hostHandler.postDelayed(this::resumeAfterClose,100);return;}
        session.clearResumeAfterPause();startSavedSession();
    }
    private String scheduleResumeRetry(String reason){
        int retry=session.incrementRetry();
        session.markRetryWaiting();
        long waitMs=Math.min(30000L,2000L<<(Math.min(retry-1,4)));
        hostHandler.removeCallbacks(resumeRetry);hostHandler.postDelayed(resumeRetry,waitMs);
        setConnection(getString(R.string.connection_reconnecting),0xFFFFD180);
        return getString(R.string.retry_countdown,reason,waitMs/1000,retry);
    }
    @Override protected void onNewIntent(Intent intent){super.onNewIntent(intent);setIntent(intent);String result=ApkUpdate.handleInstallResult(this,intent);if(result!=null)showStatus(result);}
    private final Runnable resumeRetry=new Runnable(){@Override public void run(){
        if(session.isDestroyed()||!session.isResumed()||savedAccount==null)return;
        if(NativeSession.isActive()||session.isStarting()){hostHandler.postDelayed(this,500);return;}
        startSavedSession();
    }};
    @Override protected void onResume(){super.onResume();session.setResumed(true);audioFocused=audioManager.requestAudioFocus(audioFocusRequest)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED;refreshControllerOverlay();if(game!=null)game.start();if(awaitingInstallPermission){awaitingInstallPermission=false;if(getPackageManager().canRequestPackageInstalls())openApkInstaller();else showStatus(getString(R.string.install_permission));}if(session.takeAutoResumeIfReady(NativeSession.isActive()||session.isStarting()))startSavedSession();else if(session.retryCount()>0)hostHandler.post(resumeRetry);else resumeAfterClose();}
    @Override public void onWindowFocusChanged(boolean hasFocus){super.onWindowFocusChanged(hasFocus);inputFocused=hasFocus;if(hasFocus && cooperative)enterFullscreen();if(!hasFocus){controller.clear();if(game!=null)game.keys=0;}}
    private void handleBack(){
        if(cooperative){
            if(touchOverlay.isEditing()){touchOverlay.setEditing(false);settingsOpen=false;Toast.makeText(this,R.string.position_saved,Toast.LENGTH_SHORT).show();}
            showMenu();
        }else finish();
    }
    // Android 13+ uses the registered OnBackInvokedCallback; this handles older devices.
    @SuppressLint("GestureBackNavigation")
    @Override public void onBackPressed(){handleBack();}
    @Override protected void onPause(){boolean wasStarting=session.isStarting();session.setResumed(false);audioFocused=false;audioManager.abandonAudioFocusRequest(audioFocusRequest);hostHandler.removeCallbacks(resumeRetry);controller.clear();if(game!=null){game.keys=0;if(wasStarting){session.requestResumeAfterPause();NativeSession.stop();}else if(!NativeSession.isActive())game.stop();}super.onPause();}
    @Override protected void onDestroy(){session.markDestroyed();hostHandler.removeCallbacks(resumeRetry);hostHandler.removeCallbacks(hideGameStatus);unregisterReceiver(noisyAudio);if(Build.VERSION.SDK_INT>=33 && backCallback!=null)getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback(backCallback);inputManager.unregisterInputDeviceListener(controllerDevices);NativeSession.stop();worker.shutdown();super.onDestroy();}
    @Override public boolean isResumed(){return session.isResumed();}
    @Override public boolean hasInputFocus(){return inputFocused;}
    @Override public boolean hasAudioFocus(){return audioFocused;}
    @Override public boolean isStarting(){return session.isStarting();}
    @Override public boolean isSettingsOpen(){return settingsOpen;}
    @Override public boolean isIntegerScale(){return integerScale;}
    @Override public boolean showStats(){return showStats;}
    @Override public BridgeConnection connection(){return connection;}
    @Override public void onStats(int fps){runOnUiThread(()->{if(debugStatus!=null)debugStatus.setText(getString(R.string.debug_stats,fps,pingMs>=0?pingMs+" ms":"—"));});}
    @Override public void onRendererError(Exception error){Log.e("HoennGame","Bridge stopped",error);NativeSession.stop();runOnUiThread(()->showStatus(getString(R.string.operation_failed)));}
}
