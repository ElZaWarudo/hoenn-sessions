package io.hoenn.sessions;

import android.app.Activity;
import android.app.Dialog;
import android.content.Context;
import android.content.SharedPreferences;
import android.os.Bundle;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.Window;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.TextView;
import java.util.HashSet;
import java.util.Locale;
import java.util.Set;

/** The dialog owns its window's controller events; testing never presses game keys. */
final class ControllerSettingsDialog extends Dialog {
    private static final String PREFERENCES = "controller_settings";
    private static final String[] LABELS = {"A", "B", "SELECT", "START", "Derecha", "Izquierda", "Arriba", "Abajo", "R", "L"};
    private final ControllerInput controller;
    private final Runnable onDismiss;
    private final Button[] bindingButtons = new Button[ControllerInput.ACTIONS.length];
    private TextView inputDisplay, deadZoneLabel;
    private SeekBar deadZoneSlider;
    private CheckBox stickToggle, testToggle;
    private int captureAction;
    // Keep the captured press (including repeats and release) out of dialog navigation.
    private int capturedKey = KeyEvent.KEYCODE_UNKNOWN;

    ControllerSettingsDialog(Activity activity, ControllerInput controller, Runnable onDismiss) {
        super(activity);
        this.controller = controller;
        this.onDismiss = onDismiss;
        setOnDismissListener(dialog -> {
            controller.clear();
            if (this.onDismiss != null) this.onDismiss.run();
        });
    }

    static void loadPreferences(Context context, ControllerInput controller) {
        SharedPreferences preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
        controller.resetDefaults();
        int[] keys = new int[ControllerInput.ACTIONS.length];
        Set<Integer> unique = new HashSet<>();
        boolean valid = true;
        for (int i = 0; i < keys.length; i++) {
            int action = ControllerInput.ACTIONS[i];
            keys[i] = preferences.getInt("action_" + action, controller.keyCodeFor(action));
            valid &= keys[i] > KeyEvent.KEYCODE_UNKNOWN && keys[i] <= KeyEvent.getMaxKeyCode() && unique.add(keys[i]);
        }
        if (valid) for (int i = 0; i < keys.length; i++) controller.remap(ControllerInput.ACTIONS[i], keys[i]);
        controller.setDeadZone(preferences.getFloat("dead_zone", ControllerInput.DEFAULT_DEAD_ZONE));
        controller.setLeftStickEnabled(preferences.getBoolean("left_stick", true));
    }

    private void savePreferences() {
        SharedPreferences.Editor editor = getContext().getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE).edit();
        for (int action : ControllerInput.ACTIONS) editor.putInt("action_" + action, controller.keyCodeFor(action));
        editor.putFloat("dead_zone", controller.deadZone());
        editor.putBoolean("left_stick", controller.leftStickEnabled());
        editor.apply();
    }

    @Override protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        requestWindowFeature(Window.FEATURE_NO_TITLE);
        ScrollView scroll = new ScrollView(getContext());
        LinearLayout content = new LinearLayout(getContext());
        content.setOrientation(LinearLayout.VERTICAL);
        int padding = (int) (20 * getContext().getResources().getDisplayMetrics().density);
        content.setPadding(padding, padding, padding, padding);
        scroll.addView(content);
        TextView title = new TextView(getContext());
        title.setText("Configuración del mando");
        title.setTextSize(22);
        content.addView(title);
        TextView help = new TextView(getContext());
        help.setText("Elige una acción y pulsa el botón del mando que quieras asignar. Si ya está asignado, ambas acciones intercambian botones. Los cambios se guardan automáticamente.\nActiva la prueba para ver entradas sin enviarlas al juego. Durante la prueba o la asignación, usa la pantalla para cancelar o cerrar.");
        content.addView(help);
        inputDisplay = new TextView(getContext());
        inputDisplay.setText("Sin entrada del mando.");
        inputDisplay.setAccessibilityLiveRegion(TextView.ACCESSIBILITY_LIVE_REGION_POLITE);
        content.addView(inputDisplay);
        for (int i = 0; i < bindingButtons.length; i++) {
            final int index = i;
            Button button = new Button(getContext());
            bindingButtons[i] = button;
            button.setOnClickListener(view -> {
                controller.clear();
                captureAction = ControllerInput.ACTIONS[index];
                inputDisplay.setText("Pulsa un botón del mando para " + LABELS[index] + ".");
            });
            content.addView(button);
        }
        Button cancelCapture = new Button(getContext());
        cancelCapture.setText("Cancelar asignación");
        cancelCapture.setOnClickListener(view -> {
            captureAction = 0;
            controller.clear();
            inputDisplay.setText("Asignación cancelada.");
        });
        content.addView(cancelCapture);
        deadZoneLabel = new TextView(getContext());
        content.addView(deadZoneLabel);
        deadZoneSlider = new SeekBar(getContext());
        deadZoneSlider.setMax(40);
        deadZoneSlider.setContentDescription("Zona muerta del mando, de 10 a 50 por ciento");
        deadZoneSlider.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override public void onProgressChanged(SeekBar bar, int progress, boolean fromUser) {
                if (fromUser) {
                    controller.setDeadZone((progress + 10) / 100f);
                    savePreferences();
                    updateDeadZoneLabel();
                }
            }
            @Override public void onStartTrackingTouch(SeekBar bar) { }
            @Override public void onStopTrackingTouch(SeekBar bar) { }
        });
        content.addView(deadZoneSlider);
        stickToggle = new CheckBox(getContext());
        stickToggle.setText("Usar stick izquierdo (la cruceta sigue activa)");
        stickToggle.setOnCheckedChangeListener((button, checked) -> {
            controller.setLeftStickEnabled(checked);
            savePreferences();
        });
        content.addView(stickToggle);
        testToggle = new CheckBox(getContext());
        testToggle.setText("Probar entradas del mando");
        testToggle.setOnCheckedChangeListener((button, checked) -> {
            controller.clear();
            inputDisplay.setText(checked ? "Pulsa botones o mueve el stick y la cruceta." : "Prueba desactivada.");
        });
        content.addView(testToggle);
        Button reset = new Button(getContext());
        reset.setText("Restablecer valores predeterminados");
        reset.setOnClickListener(view -> {
            captureAction = 0;
            controller.resetDefaults();
            refreshControls();
            savePreferences();
            inputDisplay.setText("Valores predeterminados restablecidos.");
        });
        content.addView(reset);
        Button close = new Button(getContext());
        close.setText("Cerrar");
        close.setOnClickListener(view -> dismiss());
        content.addView(close);
        refreshControls();
        setContentView(scroll);
    }

    @Override protected void onStart() {
        super.onStart();
        controller.clear();
        getWindow().setLayout(-1, -2);
    }

    private void refreshControls() {
        for (int i = 0; i < bindingButtons.length; i++) {
            bindingButtons[i].setText(LABELS[i] + ": " + KeyEvent.keyCodeToString(controller.keyCodeFor(ControllerInput.ACTIONS[i])));
        }
        deadZoneSlider.setProgress(Math.round(controller.deadZone() * 100) - 10);
        updateDeadZoneLabel();
        stickToggle.setChecked(controller.leftStickEnabled());
    }

    private void updateDeadZoneLabel() {
        deadZoneLabel.setText("Zona muerta: " + Math.round(controller.deadZone() * 100) + "%");
    }

    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        if (!ControllerInput.isController(event)) return super.dispatchKeyEvent(event);
        int key = event.getKeyCode();
        if (key == capturedKey) {
            if (event.getAction() == KeyEvent.ACTION_UP) capturedKey = KeyEvent.KEYCODE_UNKNOWN;
            return true;
        }
        if (captureAction != 0) {
            if (event.getAction() == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0 && key > KeyEvent.KEYCODE_UNKNOWN) {
                controller.remap(captureAction, key);
                captureAction = 0;
                capturedKey = key;
                savePreferences();
                refreshControls();
                inputDisplay.setText("Asignado: " + KeyEvent.keyCodeToString(key));
            }
            return true;
        }
        if (testToggle != null && testToggle.isChecked()) {
            int action = controller.mappedButton(key);
            String label = "sin asignar";
            for (int i = 0; i < LABELS.length; i++) if (ControllerInput.ACTIONS[i] == action) label = LABELS[i];
            inputDisplay.setText(KeyEvent.keyCodeToString(key) + " → " + label
                + (event.getAction() == KeyEvent.ACTION_UP ? " · liberado" : " · pulsado"));
            return true;
        }
        super.dispatchKeyEvent(event);
        return true;
    }

    @Override public boolean dispatchGenericMotionEvent(MotionEvent event) {
        if (!event.isFromSource(InputDevice.SOURCE_JOYSTICK)
                && !event.isFromSource(InputDevice.SOURCE_GAMEPAD)
                && !event.isFromSource(InputDevice.SOURCE_DPAD)) return super.dispatchGenericMotionEvent(event);
        if (captureAction == 0 && testToggle != null && testToggle.isChecked()) {
            inputDisplay.setText(String.format(Locale.getDefault(), "Stick: X %.2f · Y %.2f\nCruceta: X %.2f · Y %.2f",
                event.getAxisValue(MotionEvent.AXIS_X), event.getAxisValue(MotionEvent.AXIS_Y),
                event.getAxisValue(MotionEvent.AXIS_HAT_X), event.getAxisValue(MotionEvent.AXIS_HAT_Y)));
        }
        return true;
    }
}
