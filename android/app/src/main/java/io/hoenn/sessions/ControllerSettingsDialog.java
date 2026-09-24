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
    private final String[] labels;
    private static final int[] BINDING_ACTIONS = {1,2,4,8,16,32,64,128,256,512,ControllerInput.MENU,ControllerInput.FAST_FORWARD};
    private final ControllerInput controller;
    private final Runnable onDismiss;
    private final Button[] bindingButtons = new Button[BINDING_ACTIONS.length];
    private TextView inputDisplay, deadZoneLabel;
    private SeekBar deadZoneSlider;
    private CheckBox stickToggle, testToggle;
    private int captureAction;
    // Keep the captured press (including repeats and release) out of dialog navigation.
    private int capturedKey = KeyEvent.KEYCODE_UNKNOWN;

    ControllerSettingsDialog(Activity activity, ControllerInput controller, Runnable onDismiss) {
        super(activity);
        labels=new String[]{"A","B","SELECT","START",activity.getString(R.string.controller_right),activity.getString(R.string.controller_left),activity.getString(R.string.controller_up),activity.getString(R.string.controller_down),"R","L",activity.getString(R.string.controller_menu),activity.getString(R.string.controller_fast_forward)};
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
        int[] keys = new int[BINDING_ACTIONS.length];
        Set<Integer> unique = new HashSet<>();
        boolean valid = true;
        for (int i = 0; i < keys.length; i++) {
            int action = BINDING_ACTIONS[i];
            keys[i] = preferences.getInt("action_" + action, controller.keyCodeFor(action));
            valid &= keys[i] > KeyEvent.KEYCODE_UNKNOWN && keys[i] <= KeyEvent.getMaxKeyCode() && unique.add(keys[i]);
        }
        if (valid) for (int i = 0; i < keys.length; i++) controller.remap(BINDING_ACTIONS[i], keys[i]);
        controller.setDeadZone(preferences.getFloat("dead_zone", ControllerInput.DEFAULT_DEAD_ZONE));
        controller.setLeftStickEnabled(preferences.getBoolean("left_stick", true));
    }

    private void savePreferences() {
        SharedPreferences.Editor editor = getContext().getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE).edit();
        for (int action : BINDING_ACTIONS) editor.putInt("action_" + action, controller.keyCodeFor(action));
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
        title.setText(R.string.controller_settings);
        title.setTextSize(22);
        content.addView(title);
        TextView help = new TextView(getContext());
        help.setText(R.string.controller_help);
        content.addView(help);
        inputDisplay = new TextView(getContext());
        inputDisplay.setText(R.string.controller_no_input);
        inputDisplay.setAccessibilityLiveRegion(TextView.ACCESSIBILITY_LIVE_REGION_POLITE);
        content.addView(inputDisplay);
        for (int i = 0; i < bindingButtons.length; i++) {
            final int index = i;
            Button button = new Button(getContext());
            bindingButtons[i] = button;
            button.setOnClickListener(view -> {
                controller.clear();
                captureAction = BINDING_ACTIONS[index];
                inputDisplay.setText(getContext().getString(R.string.controller_capture,labels[index]));
            });
            content.addView(button);
        }
        Button cancelCapture = new Button(getContext());
        cancelCapture.setText(R.string.controller_cancel_capture);
        cancelCapture.setOnClickListener(view -> {
            captureAction = 0;
            controller.clear();
            inputDisplay.setText(R.string.controller_capture_cancelled);
        });
        content.addView(cancelCapture);
        deadZoneLabel = new TextView(getContext());
        content.addView(deadZoneLabel);
        deadZoneSlider = new SeekBar(getContext());
        deadZoneSlider.setMax(40);
        deadZoneSlider.setContentDescription(getContext().getString(R.string.controller_dead_zone_description));
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
        stickToggle.setText(R.string.controller_left_stick);
        stickToggle.setOnCheckedChangeListener((button, checked) -> {
            controller.setLeftStickEnabled(checked);
            savePreferences();
        });
        content.addView(stickToggle);
        testToggle = new CheckBox(getContext());
        testToggle.setText(R.string.controller_test);
        testToggle.setOnCheckedChangeListener((button, checked) -> {
            controller.clear();
            inputDisplay.setText(checked ? R.string.controller_test_prompt : R.string.controller_test_off);
        });
        content.addView(testToggle);
        Button reset = new Button(getContext());
        reset.setText(R.string.controller_reset);
        reset.setOnClickListener(view -> {
            captureAction = 0;
            controller.resetDefaults();
            refreshControls();
            savePreferences();
            inputDisplay.setText(R.string.controller_reset_done);
        });
        content.addView(reset);
        Button close = new Button(getContext());
        close.setText(R.string.close);
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
            bindingButtons[i].setText(getContext().getString(R.string.controller_binding,labels[i],KeyEvent.keyCodeToString(controller.keyCodeFor(BINDING_ACTIONS[i]))));
        }
        deadZoneSlider.setProgress(Math.round(controller.deadZone() * 100) - 10);
        updateDeadZoneLabel();
        stickToggle.setChecked(controller.leftStickEnabled());
    }

    private void updateDeadZoneLabel() {
        deadZoneLabel.setText(getContext().getString(R.string.controller_dead_zone,Math.round(controller.deadZone() * 100)));
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
                inputDisplay.setText(getContext().getString(R.string.controller_assigned,KeyEvent.keyCodeToString(key)));
            }
            return true;
        }
        if (testToggle != null && testToggle.isChecked()) {
            int action = controller.mappedButton(key);
            String label = getContext().getString(R.string.controller_unassigned);
            for (int i = 0; i < labels.length; i++) if (BINDING_ACTIONS[i] == action) label = labels[i];
            inputDisplay.setText(KeyEvent.keyCodeToString(key) + " → " + label
                + (event.getAction() == KeyEvent.ACTION_UP ? getContext().getString(R.string.controller_released) : getContext().getString(R.string.controller_pressed)));
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
            inputDisplay.setText(getContext().getString(R.string.controller_axes,
                event.getAxisValue(MotionEvent.AXIS_X), event.getAxisValue(MotionEvent.AXIS_Y),
                event.getAxisValue(MotionEvent.AXIS_HAT_X), event.getAxisValue(MotionEvent.AXIS_HAT_Y)));
        }
        return true;
    }
}
