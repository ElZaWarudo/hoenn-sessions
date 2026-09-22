package io.hoenn.sessions;

import android.app.Activity;
import android.app.AlertDialog;
import android.widget.CheckBox;
import android.widget.LinearLayout;
import android.widget.SeekBar;
import android.widget.TextView;

final class TouchOverlaySettingsDialog {
    private TouchOverlaySettingsDialog() { }

    static void show(Activity activity, TouchOverlay overlay, Runnable editPositions) {
        LinearLayout content=new LinearLayout(activity);content.setOrientation(LinearLayout.VERTICAL);
        int padding=(int)(20*activity.getResources().getDisplayMetrics().density);content.setPadding(padding,padding,padding,padding);
        CheckBox enabled=new CheckBox(activity);enabled.setText("Mostrar controles táctiles");enabled.setChecked(overlay.controlsVisible());content.addView(enabled);
        TextView size=new TextView(activity);content.addView(size);
        SeekBar scale=new SeekBar(activity);scale.setMax(80);scale.setProgress(Math.round((overlay.controlScale()-.65f)*100));content.addView(scale);
        Runnable update=()->size.setText("Tamaño: "+Math.round(overlay.controlScale()*100)+"%");update.run();
        enabled.setOnCheckedChangeListener((button,checked)->overlay.setControlsVisible(checked));
        scale.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){
            @Override public void onProgressChanged(SeekBar bar,int progress,boolean fromUser){if(fromUser){overlay.setControlScale(.65f+progress/100f);update.run();}}
            @Override public void onStartTrackingTouch(SeekBar bar){}
            @Override public void onStopTrackingTouch(SeekBar bar){}
        });
        AlertDialog dialog=new AlertDialog.Builder(activity).setTitle("Controles en pantalla").setView(content)
            .setNeutralButton("Restablecer",(d,w)->overlay.resetPositions())
            .setNegativeButton("Cerrar",null)
            .setPositiveButton("Mover controles",null).create();
        dialog.setOnShowListener(ignored->dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v->{dialog.dismiss();editPositions.run();}));
        dialog.show();
    }
}
