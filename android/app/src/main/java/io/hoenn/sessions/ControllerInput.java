package io.hoenn.sessions;

import android.view.KeyEvent;
import android.view.InputDevice;

final class ControllerInput {
    static final float DEFAULT_DEAD_ZONE = 0.25f;
    static final int MENU = 1024, FAST_FORWARD = 2048;
    static final int[] ACTIONS = {1, 2, 4, 8, 16, 32, 64, 128, 256, 512};
    private static final int[] DEFAULT_KEYS = {
        KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B,
        KeyEvent.KEYCODE_BUTTON_SELECT, KeyEvent.KEYCODE_BUTTON_START,
        KeyEvent.KEYCODE_DPAD_RIGHT, KeyEvent.KEYCODE_DPAD_LEFT,
        KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
        KeyEvent.KEYCODE_BUTTON_R1, KeyEvent.KEYCODE_BUTTON_L1
    };
    private final int[] bindings = DEFAULT_KEYS.clone();
    private int menuKey = KeyEvent.KEYCODE_BUTTON_MODE, fastForwardKey = KeyEvent.KEYCODE_BUTTON_R2;
    private float deadZone = DEFAULT_DEAD_ZONE;
    private boolean leftStickEnabled = true;
    private static final int RIGHT = 16, LEFT = 32, UP = 64, DOWN = 128;
    private int pressedButtons;
    private int axes;
    private int deviceId = -1;

    static boolean isController(KeyEvent event) {
        return event.isFromSource(InputDevice.SOURCE_GAMEPAD)
            || event.isFromSource(InputDevice.SOURCE_JOYSTICK)
            || event.isFromSource(InputDevice.SOURCE_DPAD);
    }

    synchronized boolean key(int sourceDeviceId, int keyCode, boolean down) {
        int mask = mappedButton(keyCode);
        if (mask == 0) return false;
        selectDevice(sourceDeviceId);
        if (down) pressedButtons |= mask;
        else pressedButtons &= ~mask;
        return true;
    }

    synchronized void axes(int sourceDeviceId, float x, float y, float hatX, float hatY) {
        selectDevice(sourceDeviceId);
        axes = (leftStickEnabled ? directions(x, y) : 0) | directions(hatX, hatY);
    }

    private void selectDevice(int sourceDeviceId) {
        if (deviceId != sourceDeviceId) {
            clear();
            deviceId = sourceDeviceId;
        }
    }

    synchronized void deviceRemoved(int sourceDeviceId) {
        if (deviceId == sourceDeviceId) clear();
    }

    private int directions(float x, float y) {
        int result = 0;
        if (x < -deadZone) result |= LEFT;
        if (x > deadZone) result |= RIGHT;
        if (y < -deadZone) result |= UP;
        if (y > deadZone) result |= DOWN;
        return result;
    }

    synchronized int keys() {
        return axes | (pressedButtons & 1023);
    }

    synchronized boolean menuPressed(){return (pressedButtons&MENU)!=0;}
    synchronized boolean fastForwardHeld(){return (pressedButtons&FAST_FORWARD)!=0;}

    synchronized int mappedButton(int keyCode) {
        if(menuKey==keyCode)return MENU;
        if(fastForwardKey==keyCode)return FAST_FORWARD;
        for (int i = 0; i < bindings.length; i++) {
            if (bindings[i] == keyCode) return ACTIONS[i];
        }
        return 0;
    }

    synchronized int keyCodeFor(int action) {
        if(action==MENU)return menuKey;
        if(action==FAST_FORWARD)return fastForwardKey;
        return bindings[actionIndex(action)];
    }

    synchronized void remap(int action, int keyCode) {
        if (keyCode <= KeyEvent.KEYCODE_UNKNOWN) throw new IllegalArgumentException("Invalid key code");
        int previous = keyCodeFor(action);
        for (int i = 0; i < bindings.length; i++) {
            if (bindings[i] == keyCode) bindings[i] = previous;
        }
        if(menuKey==keyCode)menuKey=previous;
        if(fastForwardKey==keyCode)fastForwardKey=previous;
        if(action==MENU)menuKey=keyCode;
        else if(action==FAST_FORWARD)fastForwardKey=keyCode;
        else bindings[actionIndex(action)] = keyCode;
        clear();
    }

    private static int actionIndex(int action) {
        for (int i = 0; i < ACTIONS.length; i++) if (ACTIONS[i] == action) return i;
        throw new IllegalArgumentException("Unknown GBA action");
    }

    synchronized float deadZone() { return deadZone; }

    synchronized void setDeadZone(float value) {
        if (Float.isNaN(value) || Float.isInfinite(value)) value = DEFAULT_DEAD_ZONE;
        deadZone = Math.max(0.1f, Math.min(0.5f, value));
        clear();
    }

    synchronized boolean leftStickEnabled() { return leftStickEnabled; }

    synchronized void setLeftStickEnabled(boolean enabled) {
        leftStickEnabled = enabled;
        clear();
    }

    synchronized void resetDefaults() {
        System.arraycopy(DEFAULT_KEYS, 0, bindings, 0, bindings.length);
        menuKey=KeyEvent.KEYCODE_BUTTON_MODE;fastForwardKey=KeyEvent.KEYCODE_BUTTON_R2;
        deadZone = DEFAULT_DEAD_ZONE;
        leftStickEnabled = true;
        clear();
    }

    synchronized void clear() {
        pressedButtons = 0;
        axes = 0;
        deviceId = -1;
    }
}
