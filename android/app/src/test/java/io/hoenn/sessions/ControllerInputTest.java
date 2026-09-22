package io.hoenn.sessions;

import android.view.KeyEvent;
import org.junit.Test;
import static org.junit.Assert.*;

public class ControllerInputTest {
    @Test public void remapReleasesHeldInputAndReplacesOldBinding() {
        ControllerInput input = new ControllerInput();
        input.key(7, KeyEvent.KEYCODE_BUTTON_A, true);
        input.axes(7, 1, 0, 0, 0);
        input.remap(1, KeyEvent.KEYCODE_BUTTON_X);
        assertEquals(0, input.keys());
        assertFalse(input.key(7, KeyEvent.KEYCODE_BUTTON_A, true));
        assertTrue(input.key(7, KeyEvent.KEYCODE_BUTTON_X, true));
        assertEquals(1, input.keys());
        input.key(7, KeyEvent.KEYCODE_BUTTON_X, false);
        assertEquals(0, input.keys());
    }

    @Test public void duplicateBindingSwapsPreviousKeys() {
        ControllerInput input = new ControllerInput();
        input.remap(1, KeyEvent.KEYCODE_BUTTON_B);
        assertEquals(KeyEvent.KEYCODE_BUTTON_B, input.keyCodeFor(1));
        assertEquals(KeyEvent.KEYCODE_BUTTON_A, input.keyCodeFor(2));
        input.key(7, KeyEvent.KEYCODE_BUTTON_A, true);
        assertEquals(2, input.keys());
        input.key(7, KeyEvent.KEYCODE_BUTTON_B, true);
        assertEquals(3, input.keys());
        input.remap(1, KeyEvent.KEYCODE_BUTTON_B);
        assertEquals(KeyEvent.KEYCODE_BUTTON_A, input.keyCodeFor(2));
        assertEquals(0, input.keys());
    }

    @Test public void deadZoneCanBeAdjustedAndClampsInvalidValues() {
        ControllerInput input = new ControllerInput();
        input.setDeadZone(0.4f);
        input.axes(7, -0.4f, 0.4f, 0, 0);
        assertEquals(0, input.keys());
        input.axes(7, -0.41f, 0.41f, 0, 0);
        assertEquals(32 | 128, input.keys());
        input.setDeadZone(0);
        assertEquals(0, input.keys());
        assertEquals(0.1f, input.deadZone(), 0.001f);
        input.setDeadZone(1);
        assertEquals(0.5f, input.deadZone(), 0.001f);
        input.setDeadZone(Float.NaN);
        assertEquals(0.25f, input.deadZone(), 0.001f);
    }

    @Test public void disabledStickLeavesHatAndDigitalDirectionsWorking() {
        ControllerInput input = new ControllerInput();
        input.axes(7, -1, 0, 0, 0);
        input.setLeftStickEnabled(false);
        assertEquals(0, input.keys());
        input.axes(7, -1, 1, 0, -1);
        assertEquals(64, input.keys());
        input.key(7, KeyEvent.KEYCODE_DPAD_RIGHT, true);
        assertEquals(64 | 16, input.keys());
    }

    @Test public void deviceSwitchReleasesPreviousDevicesButtonsAndAxes() {
        ControllerInput input = new ControllerInput();
        input.key(7, KeyEvent.KEYCODE_BUTTON_A, true);
        input.axes(7, 1, 0, 0, 0);
        input.key(8, KeyEvent.KEYCODE_BUTTON_B, true);
        assertEquals(2, input.keys());
        input.deviceRemoved(7);
        assertEquals(2, input.keys());
        input.axes(9, 0, -1, 0, 0);
        assertEquals(64, input.keys());
        input.deviceRemoved(9);
        assertEquals(0, input.keys());
    }

    @Test public void resetRestoresMappingsAndOptionsAndReleasesInput() {
        ControllerInput input = new ControllerInput();
        input.remap(1, KeyEvent.KEYCODE_BUTTON_X);
        input.setDeadZone(0.45f);
        input.setLeftStickEnabled(false);
        input.key(7, KeyEvent.KEYCODE_BUTTON_X, true);
        input.resetDefaults();
        assertEquals(0, input.keys());
        assertEquals(KeyEvent.KEYCODE_BUTTON_A, input.keyCodeFor(1));
        assertEquals(0.25f, input.deadZone(), 0.001f);
        assertTrue(input.leftStickEnabled());
        assertFalse(input.key(7, KeyEvent.KEYCODE_BUTTON_X, true));
        assertTrue(input.key(7, KeyEvent.KEYCODE_BUTTON_A, true));
    }

    @Test public void disconnectReleasesHeldButtonsAndAxes() {
        ControllerInput input = new ControllerInput();
        input.key(7, KeyEvent.KEYCODE_BUTTON_A, true);
        input.axes(7, 1, 0, 0, 0);
        input.deviceRemoved(8);
        assertEquals(1 | 16, input.keys());
        input.deviceRemoved(7);
        assertEquals(0, input.keys());
        input.key(8, KeyEvent.KEYCODE_BUTTON_B, true);
        assertEquals(2, input.keys());
    }

    @Test public void mapsGamepadButtonsAndReleasesThem() {
        ControllerInput input = new ControllerInput();
        assertTrue(input.key(7, KeyEvent.KEYCODE_BUTTON_A, true));
        assertTrue(input.key(7, KeyEvent.KEYCODE_BUTTON_L1, true));
        assertTrue(input.key(7, KeyEvent.KEYCODE_DPAD_RIGHT, true));
        assertEquals(1 | 512 | 16, input.keys());
        assertTrue(input.key(7, KeyEvent.KEYCODE_BUTTON_A, false));
        assertEquals(512 | 16, input.keys());
        assertFalse(input.key(7, KeyEvent.KEYCODE_BUTTON_X, true));
        assertEquals(512 | 16, input.keys());
    }

    @Test public void stickAndHatRespectDeadZoneAndClear() {
        ControllerInput input = new ControllerInput();
        input.axes(7, -0.24f, 0.25f, 0, 0);
        assertEquals(0, input.keys());
        input.axes(7, -0.5f, 0, 0, -1);
        assertEquals(32 | 64, input.keys());
        input.key(7, KeyEvent.KEYCODE_BUTTON_START, true);
        input.axes(7, 0, 0, 0, 0);
        assertEquals(8, input.keys());
        input.clear();
        assertEquals(0, input.keys());
    }
}
