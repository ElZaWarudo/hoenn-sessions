package io.hoenn.sessions;

import android.app.Activity;
import android.app.AlertDialog;
import android.widget.CheckBox;
import android.widget.LinearLayout;
import android.widget.SeekBar;
import android.widget.TextView;

final class TouchOverlaySettingsDialog {
    private TouchOverlaySettingsDialog() { }

    static void show(Activity activity, TouchOverlay overlay, Runnable editPositions, Runnable dismissed) {
        LinearLayout content=new LinearLayout(activity);content.setOrientation(LinearLayout.VERTICAL);
        int padding=(int)(20*activity.getResources().getDisplayMetrics().density);content.setPadding(padding,padding,padding,padding);
        CheckBox enabled=new CheckBox(activity);enabled.setText(R.string.touch_show_controls);enabled.setChecked(overlay.controlsVisible());content.addView(enabled);
        CheckBox fastForward=new CheckBox(activity);fastForward.setText(R.string.touch_show_fast_forward);fastForward.setChecked(overlay.fastForwardEnabled());content.addView(fastForward);
        TextView size=new TextView(activity);content.addView(size);
        SeekBar scale=new SeekBar(activity);scale.setMax(80);scale.setProgress(Math.round((overlay.controlScale()-.65f)*100));content.addView(scale);
        Runnable update=()->size.setText(activity.getString(R.string.touch_size,Math.round(overlay.controlScale()*100)));update.run();
        enabled.setOnCheckedChangeListener((button,checked)->overlay.setControlsVisible(checked));
        fastForward.setOnCheckedChangeListener((button,checked)->overlay.setFastForwardEnabled(checked));
        scale.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){
            @Override public void onProgressChanged(SeekBar bar,int progress,boolean fromUser){if(fromUser){overlay.setControlScale(.65f+progress/100f);update.run();}}
            @Override public void onStartTrackingTouch(SeekBar bar){}
            @Override public void onStopTrackingTouch(SeekBar bar){}
        });
        TextView opacityLabel=new TextView(activity);content.addView(opacityLabel);
        SeekBar opacity=new SeekBar(activity);opacity.setMax(80);opacity.setProgress(Math.round(overlay.controlOpacity()*100)-20);content.addView(opacity);
        Runnable updateOpacity=()->opacityLabel.setText(activity.getString(R.string.touch_opacity,Math.round(overlay.controlOpacity()*100)));updateOpacity.run();
        opacity.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){
            @Override public void onProgressChanged(SeekBar bar,int progress,boolean fromUser){if(fromUser){overlay.setControlOpacity((progress+20)/100f);updateOpacity.run();}}
            @Override public void onStartTrackingTouch(SeekBar bar){}
            @Override public void onStopTrackingTouch(SeekBar bar){}
        });
        AlertDialog dialog=new AlertDialog.Builder(activity).setTitle(R.string.touch_settings).setView(content)
            .setNeutralButton(R.string.touch_reset,(d,w)->overlay.resetDefaults())
            .setNegativeButton(R.string.close,null)
            .setPositiveButton(R.string.touch_move,null).create();
        dialog.setOnDismissListener(ignored->dismissed.run());
        dialog.setOnShowListener(ignored->dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v->{dialog.dismiss();editPositions.run();}));
        dialog.show();
    }
}
