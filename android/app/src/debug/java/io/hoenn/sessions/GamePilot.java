package io.hoenn.sessions;

import android.app.*;
import android.content.Intent;
import android.graphics.*;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONObject;

/** Explicit debug-only operator harness. Drives production UI, never writes game memory or saves. */
public final class GamePilot extends Instrumentation {
    private MainActivity activity;
    private JSONObject credentials;
    private File root;
    @Override public void onCreate(Bundle args){super.onCreate(args);start();}
    private View find(View view,String text){
        if(view instanceof Button && ((Button)view).getText().toString().equals(text))return view;
        if(view instanceof EditText && text.equals(String.valueOf(((EditText)view).getHint())))return view;
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){View found=find(group.getChildAt(i),text);if(found!=null)return found;}}
        return null;
    }
    private View find(String text){View view=find(activity.getWindow().getDecorView(),text);if(view==null)throw new IllegalStateException("Missing control");return view;}
    private void click(String text){runOnMainSync(()->find(text).performClick());}
    private String state(){AtomicReference<String> text=new AtomicReference<>();runOnMainSync(()->{TextView status=activity.getWindow().getDecorView().findViewWithTag("session-status");text.set(status.getText().toString());});return text.get();}
    private void snapshot() throws Exception {
        AtomicReference<Bitmap> frame=new AtomicReference<>();
        runOnMainSync(()->{View view=activity.getWindow().getDecorView().findViewWithTag("gba-frame");Bitmap bitmap=Bitmap.createBitmap(Math.max(240,view.getWidth()),Math.max(160,view.getHeight()),Bitmap.Config.ARGB_8888);view.draw(new Canvas(bitmap));frame.set(bitmap);});
        try(OutputStream out=new FileOutputStream(new File(root,"pilot-frame.png"))){frame.get().compress(Bitmap.CompressFormat.PNG,100,out);}finally{frame.get().recycle();}
    }
    private void key(String key,int holdMs) throws Exception {
        if(holdMs<20 || holdMs>10000)throw new IllegalArgumentException("Invalid hold duration");
        long down=SystemClock.uptimeMillis();AtomicReference<TouchOverlay> target=new AtomicReference<>();AtomicReference<float[]> center=new AtomicReference<>();
        runOnMainSync(()->{TouchOverlay overlay=activity.getWindow().getDecorView().findViewWithTag("touch-overlay");target.set(overlay);center.set(overlay.controlCenter(key));MotionEvent event=MotionEvent.obtain(down,down,MotionEvent.ACTION_DOWN,center.get()[0],center.get()[1],0);try{overlay.dispatchTouchEvent(event);}finally{event.recycle();}});
        try{Thread.sleep(holdMs);}finally{runOnMainSync(()->{MotionEvent event=MotionEvent.obtain(down,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,center.get()[0],center.get()[1],0);try{target.get().dispatchTouchEvent(event);}finally{event.recycle();}});}
    }
    private void report(JSONObject result)throws Exception{
        File temporary=new File(root,"pilot-result.tmp"),target=new File(root,"pilot-result.json");
        try(OutputStream out=new FileOutputStream(temporary)){out.write(result.toString().getBytes(StandardCharsets.UTF_8));}
        if(!temporary.renameTo(target))throw new IOException("Report write failed");
    }
    @Override public void onStart(){
        root=getTargetContext().getFilesDir();File input=new File(root,"pilot-credentials.json");
        try{
            try(InputStream in=new FileInputStream(input)){credentials=new JSONObject(new String(CloudApi.bounded(in,4096),StandardCharsets.UTF_8));}
            if(!input.delete())throw new IOException("Credential cleanup failed");
            Intent intent=new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity=(MainActivity)startActivitySync(intent);
            report(new JSONObject().put("id",0).put("status",state()));
            long deadline=SystemClock.elapsedRealtime()+60*60*1000;
            while(SystemClock.elapsedRealtime()<deadline){
                File file=new File(root,"pilot-command.json");if(!file.exists()){Thread.sleep(100);continue;}
                JSONObject command;try(InputStream in=new FileInputStream(file)){command=new JSONObject(new String(CloudApi.bounded(in,4096),StandardCharsets.UTF_8));}
                if(!file.delete())throw new IOException("Command cleanup failed");
                String op=command.getString("op");
                switch(op){
                    case "login": runOnMainSync(()->{try{((EditText)find("Usuario")).setText(credentials.getString("username"));((EditText)find("Contraseña")).setText(credentials.getString("password"));find("Iniciar sesión y jugar").performClick();}catch(Exception e){throw new IllegalStateException("Login UI failed");}});break;

                    case "key":key(command.getString("key"),command.optInt("hold_ms",120));break;
                    case "close":runOnMainSync(()->activity.stopSession());break;
                    case "reconnect":runOnMainSync(()->activity.reconnectSession());break;
                    case "status":break;
                    case "finish":if(NativeSession.isActive())throw new IllegalStateException("Close the session before finishing");credentials=null;finish(Activity.RESULT_OK,new Bundle());return;
                    default:throw new IllegalArgumentException("Unsupported operation");
                }
                int wait=command.optInt("wait_ms",250);if(wait<0 || wait>30000)throw new IllegalArgumentException("Invalid wait");Thread.sleep(wait);
                snapshot();report(new JSONObject().put("id",command.getLong("id")).put("active",NativeSession.isActive()).put("status",state()));
            }
            throw new IOException("Pilot deadline reached");
        }catch(Throwable error){
            NativeSession.stop();
            long stopDeadline=SystemClock.elapsedRealtime()+30000;
            while(NativeSession.isActive() && SystemClock.elapsedRealtime()<stopDeadline){try{Thread.sleep(100);}catch(InterruptedException ignored){break;}}
            try{report(new JSONObject().put("error",error.getClass().getSimpleName()));}catch(Exception ignored){}
            finish(Activity.RESULT_CANCELED,new Bundle());
        }finally{credentials=null;input.delete();}
    }
}
