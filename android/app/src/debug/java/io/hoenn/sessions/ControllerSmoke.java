package io.hoenn.sessions;

import android.app.*;
import android.content.*;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.util.Map;

/** Explicit debug-only integration check; restores the user's controller preferences. */
public final class ControllerSmoke extends Instrumentation {
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    private static Button find(View view, String prefix) {
        if (view instanceof Button && ((Button)view).getText().toString().startsWith(prefix)) return (Button)view;
        if (view instanceof android.view.ViewGroup) {
            android.view.ViewGroup group=(android.view.ViewGroup)view;
            for(int i=0;i<group.getChildCount();i++){Button button=find(group.getChildAt(i),prefix);if(button!=null)return button;}
        }
        return null;
    }
    private static void check(boolean value) { if(!value)throw new AssertionError("Controller integration check failed"); }
    @Override public void onStart() {
        SharedPreferences preferences=getTargetContext().getSharedPreferences("controller_settings",Context.MODE_PRIVATE);
        Map<String,?> original=preferences.getAll();
        Bundle result=new Bundle();
        MainActivity activity=null;
        try {
            activity=(MainActivity)startActivitySync(new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final MainActivity host=activity;
            runOnMainSync(()->{
                ControllerInput input=new ControllerInput();
                ControllerSettingsDialog dialog=new ControllerSettingsDialog(host,input,()->{});
                dialog.show();
                try {
                    find(dialog.getWindow().getDecorView(),"A:").performClick();
                    dialog.dispatchKeyEvent(new KeyEvent(0,0,KeyEvent.ACTION_DOWN,KeyEvent.KEYCODE_BUTTON_X,0,0,37,0,0,InputDevice.SOURCE_GAMEPAD));
                    dialog.dispatchKeyEvent(new KeyEvent(0,1,KeyEvent.ACTION_UP,KeyEvent.KEYCODE_BUTTON_X,0,0,37,0,0,InputDevice.SOURCE_GAMEPAD));
                    check(input.keyCodeFor(1)==KeyEvent.KEYCODE_BUTTON_X && input.keys()==0);
                    ControllerInput reloaded=new ControllerInput();
                    ControllerSettingsDialog.loadPreferences(host,reloaded);
                    check(reloaded.keyCodeFor(1)==KeyEvent.KEYCODE_BUTTON_X);
                    find(dialog.getWindow().getDecorView(),"Restablecer").performClick();
                    ControllerSettingsDialog.loadPreferences(host,reloaded);
                    check(reloaded.keyCodeFor(1)==KeyEvent.KEYCODE_BUTTON_A);
                    preferences.edit().putInt("action_1",KeyEvent.KEYCODE_BUTTON_B).commit();
                    ControllerSettingsDialog.loadPreferences(host,reloaded);
                    check(reloaded.keyCodeFor(1)==KeyEvent.KEYCODE_BUTTON_A && reloaded.keyCodeFor(2)==KeyEvent.KEYCODE_BUTTON_B);
                } finally { dialog.dismiss(); }
            });
            result.putString("controller","PASS: capture, no gameplay input, persistence, reset, corrupt mapping fallback");
        } catch(Throwable error) { result.putString("failure",error.getClass().getSimpleName()); }
        finally {
            SharedPreferences.Editor restore=preferences.edit().clear();
            for(Map.Entry<String,?> entry:original.entrySet()) {
                Object value=entry.getValue();
                if(value instanceof Integer)restore.putInt(entry.getKey(),(Integer)value);
                else if(value instanceof Float)restore.putFloat(entry.getKey(),(Float)value);
                else if(value instanceof Boolean)restore.putBoolean(entry.getKey(),(Boolean)value);
            }
            restore.commit();
            if(activity!=null){final MainActivity host=activity;runOnMainSync(host::finish);}
        }
        finish(result.containsKey("failure")?Activity.RESULT_CANCELED:Activity.RESULT_OK,result);
    }
}
