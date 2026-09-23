package io.hoenn.sessions;

import android.app.*;
import android.content.*;
import android.os.*;
import android.view.MotionEvent;
import java.util.Map;

/** Debug-only integration check for the optional hold-to-run fast-forward control. */
public final class OverlaySmoke extends Instrumentation {
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    private static void check(boolean value,String message) { if(!value)throw new AssertionError(message); }
    @Override public void onStart() {
        SharedPreferences preferences=getTargetContext().getSharedPreferences("touch_overlay",Context.MODE_PRIVATE);
        Map<String,?> original=preferences.getAll();Bundle result=new Bundle();
        try {
            runOnMainSync(()->{
                TouchOverlay overlay=new TouchOverlay(getTargetContext());
                overlay.setControlsVisible(true);overlay.setFastForwardEnabled(true);overlay.setEditing(false);
                overlay.layout(0,0,2340,1080);
                float[] center=overlay.controlCenter("»");long now=SystemClock.uptimeMillis();
                MotionEvent down=MotionEvent.obtain(now,now,MotionEvent.ACTION_DOWN,center[0],center[1],0);
                overlay.dispatchTouchEvent(down);down.recycle();
                check(overlay.fastForwardHeld(),"Fast-forward did not start on press");
                check(overlay.keys()==0,"Fast-forward leaked into GBA button bits");
                MotionEvent up=MotionEvent.obtain(now,now+100,MotionEvent.ACTION_UP,center[0],center[1],0);
                overlay.dispatchTouchEvent(up);up.recycle();
                check(!overlay.fastForwardHeld(),"Fast-forward stayed active after release");
                check(!overlay.fastForwardHeld(),"Fast-forward behaved as a toggle");
            });
            result.putString("overlay","PASS: optional control activates on press and stops on release");
        } catch(Throwable error) { result.putString("failure",error.toString()); }
        finally {
            SharedPreferences.Editor restore=preferences.edit().clear();
            for(Map.Entry<String,?> entry:original.entrySet()) {
                Object value=entry.getValue();
                if(value instanceof Float)restore.putFloat(entry.getKey(),(Float)value);
                else if(value instanceof Boolean)restore.putBoolean(entry.getKey(),(Boolean)value);
            }
            restore.commit();
        }
        finish(result.containsKey("failure")?Activity.RESULT_CANCELED:Activity.RESULT_OK,result);
    }
}
