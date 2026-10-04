"""Preparation must stop credential input on focus loss and retain caches."""
import sys
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_prepare_clients as prepare


class PreparationTests(unittest.TestCase):
    def test_keyboard_focus_loss_stops_remaining_secret_characters(self):
        adapter = mock.Mock()
        adapter.user32.VkKeyScanW.side_effect = lambda char: ord(char.upper())
        adapter.foreground.side_effect = [100, 100, 200]
        with mock.patch.object(prepare.time, "sleep"):
            with self.assertRaisesRegex(prepare.HarnessFailure, "lost focus"):
                prepare.type_text("abc", adapter, 100)
        downs = [call.args[0] for call in adapter.key.call_args_list if call.args[1]]
        self.assertEqual(downs, [ord("A")])
        self.assertIn(mock.call(ord("A"), False), adapter.key.call_args_list)

    def test_reused_complete_profiles_do_not_launch_or_install_again(self):
        plan = {"players": [{"name": "a"}, {"name": "b"}]}
        with mock.patch.object(prepare, "preflight"), \
             mock.patch.object(prepare, "_paths", return_value=(Path("release"), Path("run"))), \
             mock.patch.object(prepare, "_cache_ready", return_value=True), \
             mock.patch.object(prepare, "checkpoint"), \
             mock.patch.object(prepare, "launch_signed_desktop") as launch:
            self.assertEqual(prepare.prepare(plan), {"reused": ["a", "b"], "prepared": []})
            launch.assert_not_called()


BACKGROUND, WHITE, INK, BLUE = (248, 248, 248), (255, 255, 255), (60, 60, 60), (0, 92, 128)


def geometry_user32(dpi, client=(137, 166), window=(128, 128)):
    user32 = mock.Mock()
    user32.GetDpiForWindow.return_value = dpi
    def to_screen(handle, pointer):
        pointer._obj.x += client[0]
        pointer._obj.y += client[1]
        return True
    def rect(handle, pointer):
        pointer._obj.left, pointer._obj.top = window
        pointer._obj.right, pointer._obj.bottom = window[0] + 768, window[1] + 572
        return True
    user32.ClientToScreen.side_effect = to_screen
    user32.GetWindowRect.side_effect = rect
    return user32


def form_capture(geometry, *, user=(False, 0), password=(False, 0), width=768, height=572):
    """Synthetic signed form: (focused, glyph count) per TextEdit."""
    pixels = [[BACKGROUND] * width for _ in range(height)]
    scale = geometry[0]
    for field, (focused, glyphs) in ((prepare.USERNAME_FIELD, user),
                                     (prepare.PASSWORD_FIELD, password)):
        x0, y0, x1, y1 = prepare.field_box(geometry, field)
        for y in range(y0, y1):
            for x in range(x0, x1):
                pixels[y][x] = WHITE
        if focused:
            for x in range(x0 - 1, x1 + 1):
                pixels[y0 - 1][x] = pixels[y1][x] = BLUE
        for glyph in range(glyphs):
            gx = x0 + round((8 + glyph * 6) * scale)
            for y in range(y0 + round(10 * scale), y0 + round(14 * scale)):
                for x in range(gx, gx + max(2, round(3 * scale))):
                    pixels[y][x] = INK
    return width, height, b"".join(bytes(p) for row in pixels for p in row)


class FormGeometryTests(unittest.TestCase):
    def test_coordinates_follow_window_dpi_unscaled_and_scaled(self):
        adapter = mock.Mock()
        for dpi, scale, expected in ((96, 1.0, (9 + 160, 38 + 145)),
                                     (120, 1.25, (9 + 200, 38 + 181)),
                                     (144, 1.5, (9 + 240, 38 + 218))):
            adapter.user32 = geometry_user32(dpi)
            geometry = prepare.form_geometry(1, adapter)
            self.assertEqual(geometry, (scale, 9, 38))
            self.assertEqual(prepare.to_window(geometry, 160, 145), expected)

    def test_click_form_hovers_before_press_at_scaled_point(self):
        adapter = mock.Mock()
        adapter.user32 = geometry_user32(120)
        adapter.windows.return_value = [mock.Mock(pid=7, visible=True, title="Hoenn Sessions", handle=1)]
        with mock.patch.object(prepare, "click_desktop") as click:
            self.assertEqual(prepare.click_form(7, 160, 145, adapter), 1)
        click.assert_called_once_with(7, 209, 219, adapter, settle=prepare.CLICK_SETTLE)

    def test_unavailable_dpi_fails_closed(self):
        adapter = mock.Mock()
        adapter.user32 = geometry_user32(0)
        with self.assertRaisesRegex(prepare.HarnessFailure, "DPI unavailable"):
            prepare.form_geometry(1, adapter)

    def test_field_state_reads_focus_and_glyphs_at_each_scale(self):
        for scale in (1.0, 1.25, 1.5):
            geometry = (scale, 9, 38)
            capture = form_capture(geometry, user=(False, 7), password=(True, 12))
            user = prepare.field_state(capture, prepare.field_box(geometry, prepare.USERNAME_FIELD), scale)
            secret = prepare.field_state(capture, prepare.field_box(geometry, prepare.PASSWORD_FIELD), scale)
            self.assertTrue(user["present"] and secret["present"], scale)
            self.assertEqual((user["focused"], secret["focused"]), (False, True))
            self.assertEqual((user["runs"], secret["runs"]), (7, 12))
            self.assertFalse(user["empty"] or secret["empty"])

    def test_wrong_scale_does_not_recognize_the_form(self):
        actual = (1.25, 9, 38)
        capture = form_capture(actual)
        assumed = (1.0, 9, 38)
        state = prepare.field_state(capture, prepare.field_box(assumed, prepare.USERNAME_FIELD), 1.0)
        self.assertFalse(state["present"])


class CredentialEntryTests(unittest.TestCase):
    def run_entry(self, states, password="pw4567"):
        adapter = mock.Mock()
        adapter.user32.VkKeyScanW.side_effect = lambda char: ord(char.upper())
        typed = []
        focus = []
        def focus_field(pid, field, _adapter):
            focus.append(field)
            return 1, states[min(len(focus), len(states)) - 1]
        with mock.patch.object(prepare, "_signed_handle", return_value=1), \
             mock.patch.object(prepare, "form_states", side_effect=states), \
             mock.patch.object(prepare, "focus_field", side_effect=focus_field), \
             mock.patch.object(prepare, "type_text", side_effect=lambda value, *_: typed.append(value)), \
             mock.patch.object(prepare.time, "sleep"):
            try:
                return prepare.enter_credentials(7, "HarnessUser", password, adapter), typed, focus
            except prepare.HarnessFailure as exc:
                exc.typed, exc.focus = typed, focus
                raise

    @staticmethod
    def state(ink=0, runs=0, focused=False):
        return {"present": True, "focused": focused, "ink": ink, "runs": runs,
                "empty": ink == 0}

    def test_entered_username_and_matching_glyphs_allow_submit(self):
        empty = (self.state(), self.state())
        typed_user = (self.state(500, 11), self.state())
        handle, typed, focus = self.run_entry([empty, typed_user, (self.state(500, 11), self.state(60, 7))])
        self.assertEqual(handle, 1)
        self.assertEqual(typed, ["HarnessUser", "pw4567"])
        self.assertEqual(focus, [prepare.USERNAME_FIELD, prepare.PASSWORD_FIELD])

    def test_missing_username_fails_closed_before_any_password_key(self):
        empty = (self.state(), self.state())
        with self.assertRaisesRegex(prepare.HarnessFailure, "username was not entered") as caught:
            self.run_entry([empty, empty])
        self.assertEqual(caught.exception.typed, ["HarnessUser"])
        self.assertFalse(getattr(caught.exception, "password_typed", False))
        self.assertNotIn("pw4567", str(caught.exception))

    def test_username_landing_in_password_field_fails_before_password(self):
        empty = (self.state(), self.state())
        wrong = (self.state(), self.state(500, 11))
        with self.assertRaisesRegex(prepare.HarnessFailure, "username was not entered") as caught:
            self.run_entry([empty, wrong])
        self.assertEqual(caught.exception.typed, ["HarnessUser"])

    def test_password_glyph_mismatch_fails_closed_without_secret_in_message(self):
        empty = (self.state(), self.state())
        typed_user = (self.state(500, 11), self.state())
        for runs in (0, 5, 8):
            with self.assertRaisesRegex(prepare.HarnessFailure, "refusing to submit") as caught:
                self.run_entry([empty, typed_user, (self.state(500, 11), self.state(60, runs))])
            self.assertTrue(caught.exception.password_typed)
            self.assertNotIn("pw4567", str(caught.exception))
            self.assertNotIn("6", str(caught.exception))

    def test_username_change_during_password_fails_closed(self):
        empty = (self.state(), self.state())
        typed_user = (self.state(500, 11), self.state())
        with self.assertRaisesRegex(prepare.HarnessFailure, "username changed") as caught:
            self.run_entry([empty, typed_user, (self.state(800, 15), self.state(60, 6))])
        self.assertTrue(caught.exception.password_typed)

    def test_unsupported_password_character_stops_before_any_typing(self):
        adapter = mock.Mock()
        adapter.user32.VkKeyScanW.side_effect = lambda char: -1 if char == "\u00e9" else 65
        with mock.patch.object(prepare, "type_text") as type_text, \
             mock.patch.object(prepare, "form_states") as states:
            with self.assertRaisesRegex(prepare.HarnessFailure, "keyboard layout"):
                prepare.enter_credentials(7, "HarnessUser", "caf\u00e9", adapter)
        type_text.assert_not_called()
        states.assert_not_called()

    def test_non_empty_form_is_never_typed_into(self):
        with self.assertRaisesRegex(prepare.HarnessFailure, "not empty") as caught:
            self.run_entry([(self.state(500, 11), self.state())])
        self.assertEqual(caught.exception.typed, [])

    def test_focus_field_retries_then_fails_without_typing(self):
        unfocused = (self.state(), self.state())
        with mock.patch.object(prepare, "click_form", return_value=1) as click, \
             mock.patch.object(prepare, "form_states", return_value=unfocused), \
             mock.patch.object(prepare.time, "sleep"):
            with self.assertRaisesRegex(prepare.HarnessFailure, "username field did not take focus"):
                prepare.focus_field(7, prepare.USERNAME_FIELD, mock.Mock())
        self.assertEqual(click.call_count, prepare.FOCUS_ATTEMPTS)

    def test_focus_field_rejects_both_fields_focused(self):
        both = (self.state(focused=True), self.state(focused=True))
        only_secret = (self.state(), self.state(focused=True))
        with mock.patch.object(prepare, "click_form", return_value=1), \
             mock.patch.object(prepare, "form_states", side_effect=[both, only_secret]), \
             mock.patch.object(prepare.time, "sleep"):
            handle, states = prepare.focus_field(7, prepare.PASSWORD_FIELD, mock.Mock())
        self.assertEqual((handle, states), (1, only_secret))


BUTTON = (230, 230, 230)
LIGHT_RING = (191, 212, 223)


class FakeJoinDesktop:
    """Synthetic partner panel: join TextEdit at logical (8, top) and a Join button."""

    def __init__(self, scale=1.25, top=300, *, focusable=True, typed_glyphs=7, prefilled=0,
                 extra_box=False, clear_on_join=True, move_after_focus=False, egui_focus=False):
        self.scale, self.top, self.focusable, self.typed_glyphs = scale, top, focusable, typed_glyphs
        self.focused, self.glyphs, self.extra_box = False, prefilled, extra_box
        self.clear_on_join, self.move_after_focus = clear_on_join, move_after_focus
        # egui 0.31: the focused frame expands by one point and strokes inside,
        # so its anti-aliased edge covers the interior's outer pixels and a
        # caret spans the text row (what the live v8b desktop showed).
        self.egui_focus = egui_focus
        self.geometry = (scale, 9, 38)
        self.clicks, self.typed, self.joins = [], [], 0

    def box(self, top=None):
        s, left, client = self.geometry
        top = self.top if top is None else top
        return (left + round(8 * s), client + round(top * s), left + round(112 * s), client + round((top + 24) * s))

    def button(self):
        x0, y0, x1, y1 = self.box()
        return x1 + round(8 * self.scale), y0, x1 + round(42 * self.scale), y1

    def capture(self, width=768, height=760):
        s = self.scale
        pixels = [[BACKGROUND] * width for _ in range(height)]
        boxes = [self.box()] + ([self.box(self.top + 60)] if self.extra_box else [])
        for x0, y0, x1, y1 in boxes:
            for y in range(y0, y1):
                for x in range(x0, x1):
                    pixels[y][x] = WHITE
        x0, y0, x1, y1 = self.box()
        if self.focused and self.egui_focus:
            for x in range(x0 - 1, x1 + 1):
                pixels[y0 - 1][x] = pixels[y1][x] = BLUE
            for y in range(y0 - 1, y1 + 1):
                pixels[y][x0 - 1] = pixels[y][x1] = BLUE
            for x in range(x0, x1):
                pixels[y0][x] = pixels[y1 - 1][x] = LIGHT_RING
            for y in range(y0, y1):
                pixels[y][x0] = pixels[y][x1 - 1] = LIGHT_RING
            for y in range(y0 + 2, y1 - 2):
                pixels[y][x0 + 3] = pixels[y][x0 + 4] = BLUE
        elif self.focused:
            for x in range(x0 - 1, x1 + 1):
                pixels[y0 - 1][x] = pixels[y1][x] = BLUE
        for glyph in range(self.glyphs):
            gx = x0 + round((4 + glyph * 7) * s)
            for y in range(y0 + round(6 * s), y0 + round(16 * s)):
                for x in range(gx, gx + max(2, round(3 * s))):
                    pixels[y][x] = INK
        bx0, by0, bx1, by1 = self.button()
        for y in range(by0, by1):
            for x in range(bx0, bx1):
                pixels[y][x] = BUTTON
        return width, height, b"".join(bytes(p) for row in pixels for p in row)

    def click(self, pid, x, y, adapter, settle=0):
        self.clicks.append((x, y))
        x0, y0, x1, y1 = self.box()
        bx0, by0, bx1, by1 = self.button()
        if x0 <= x < x1 and y0 <= y < y1:
            self.focused = self.focusable
            if self.move_after_focus:
                self.top += 40
        elif bx0 <= x < bx1 and by0 <= y < by1:
            self.joins += 1
            if self.clear_on_join and self.joins >= 1:
                self.glyphs = 0

    def type(self, value, adapter, handle):
        self.typed.append(value)
        self.glyphs = self.typed_glyphs


class JoinCodeTests(unittest.TestCase):
    def run_join(self, desktop, confirm=None, code="ABC-234"):
        adapter = mock.Mock()
        adapter.user32.VkKeyScanW.side_effect = lambda char: ord(char)
        confirm = confirm or mock.Mock(return_value=True)
        with mock.patch.object(prepare, "_focus_signed", return_value=1), \
             mock.patch.object(prepare, "form_geometry", return_value=desktop.geometry), \
             mock.patch.object(prepare, "_capture", side_effect=lambda handle, _adapter: desktop.capture()), \
             mock.patch.object(prepare, "click_desktop", side_effect=desktop.click), \
             mock.patch.object(prepare, "type_text", side_effect=desktop.type), \
             mock.patch.object(prepare.time, "sleep"), \
             mock.patch.object(prepare.time, "monotonic", side_effect=iter(range(0, 100000, 1))):
            return prepare.enter_join_code(7, code, adapter, confirm), confirm

    def test_box_and_button_are_found_at_each_scale_and_panel_height(self):
        for scale in (1.0, 1.25, 1.5):
            for top in (220, 300, 380):
                desktop = FakeJoinDesktop(scale, top)
                box = prepare.find_join_box(desktop.capture(), desktop.geometry)
                self.assertEqual(box, desktop.box())
                x, y = prepare.find_join_button(desktop.capture(), box, scale)
                bx0, by0, bx1, by1 = desktop.button()
                self.assertTrue(bx0 <= x < bx1 and by0 <= y < by1)

    def test_types_code_presses_join_and_returns_after_server_confirmation(self):
        desktop = FakeJoinDesktop()
        receipt, confirm = self.run_join(desktop)
        self.assertEqual(desktop.typed, ["ABC-234"])
        self.assertEqual((desktop.joins, receipt["join_clicks"]), (1, 1))
        confirm.assert_called()

    def test_malformed_code_sends_no_click_or_key(self):
        for code in ("abc-234", "ABC234", "ABC-23O", "ABC-2345", None):
            desktop = FakeJoinDesktop()
            with self.subTest(code=code), self.assertRaisesRegex(prepare.HarnessFailure, "malformed"):
                self.run_join(desktop, code=code)
            self.assertEqual((desktop.clicks, desktop.typed), ([], []))

    def test_ambiguous_or_missing_box_fails_before_input(self):
        desktop = FakeJoinDesktop(extra_box=True)
        with self.assertRaisesRegex(prepare.HarnessFailure, "not uniquely recognized"):
            self.run_join(desktop)
        self.assertEqual((desktop.clicks, desktop.typed), ([], []))
        blank = (768, 760, bytes(BACKGROUND) * (768 * 760))
        with self.assertRaisesRegex(prepare.HarnessFailure, "not uniquely recognized"):
            prepare.find_join_box(blank, (1.25, 9, 38))

    def test_non_empty_box_is_never_typed_into(self):
        desktop = FakeJoinDesktop(prefilled=7)
        with self.assertRaisesRegex(prepare.HarnessFailure, "not an empty TextEdit"):
            self.run_join(desktop)
        self.assertEqual((desktop.clicks, desktop.typed), ([], []))

    def test_unfocused_box_retries_then_types_nothing(self):
        desktop = FakeJoinDesktop(focusable=False)
        with self.assertRaisesRegex(prepare.HarnessFailure, "did not take focus"):
            self.run_join(desktop)
        self.assertEqual(len(desktop.clicks), prepare.FOCUS_ATTEMPTS)
        self.assertEqual(desktop.typed, [])

    def test_layout_change_stops_input(self):
        desktop = FakeJoinDesktop(move_after_focus=True)
        with self.assertRaisesRegex(prepare.HarnessFailure, "layout moved"):
            self.run_join(desktop)
        self.assertEqual(desktop.typed, [])

    def test_invisible_code_never_presses_join(self):
        desktop = FakeJoinDesktop(typed_glyphs=1)
        with self.assertRaisesRegex(prepare.HarnessFailure, "not visibly entered"):
            self.run_join(desktop)
        self.assertEqual(desktop.joins, 0)

    def test_unconfirmed_join_is_repressed_only_while_code_remains_then_fails(self):
        desktop = FakeJoinDesktop(clear_on_join=False)
        with self.assertRaisesRegex(prepare.HarnessFailure, "did not report the paired group"):
            self.run_join(desktop, mock.Mock(return_value=False))
        self.assertEqual(desktop.joins, prepare.JOIN_CLICKS)

    def test_cleared_box_without_server_group_fails_after_one_join(self):
        desktop = FakeJoinDesktop()
        with self.assertRaisesRegex(prepare.HarnessFailure, "server reports no Active group"):
            self.run_join(desktop, mock.Mock(return_value=False))
        self.assertEqual(desktop.joins, 1)

    def test_wrong_partner_from_confirmation_propagates(self):
        desktop = FakeJoinDesktop()
        confirm = mock.Mock(side_effect=prepare.HarnessFailure("join/a: Active group has an unexpected partner"))
        with self.assertRaisesRegex(prepare.HarnessFailure, "unexpected partner"):
            self.run_join(desktop, confirm)
        self.assertEqual(desktop.joins, 1)


    def test_egui_focus_inset_and_caret_are_the_same_box_at_each_scale(self):
        for scale in (1.0, 1.25, 1.5):
            desktop = FakeJoinDesktop(scale, egui_focus=True)
            with self.subTest(scale=scale):
                receipt, _confirm = self.run_join(desktop)
                self.assertEqual(desktop.typed, ["ABC-234"])
                self.assertEqual(receipt["box"], list(desktop.box()))

    def test_egui_focus_then_layout_change_still_stops_input(self):
        desktop = FakeJoinDesktop(egui_focus=True, move_after_focus=True)
        with self.assertRaisesRegex(prepare.HarnessFailure, "layout moved"):
            self.run_join(desktop)
        self.assertEqual(desktop.typed, [])


FIXTURES = Path(__file__).resolve().parent / "fixtures"
# Real capture of signed desktop b (v8b run 1db9d3f7..., outbound-join-failed-b.png)
# after the focus click: client origin (9, 38), 125% scale, empty focused box.
FOCUSED_LIVE = FIXTURES / "desktop-join-box-focused-live-125pct.png"
# Same window rows 200..400, columns 0..400 with the focus stroke and caret
# repainted to egui 0.31's unfocused/unhovered TextEdit (white, no stroke).
UNFOCUSED_CROP = FIXTURES / "desktop-join-box-unfocused-derived-125pct-crop.png"
CROP = (0, 200, 400, 400)
CROP_GEOMETRY = (1.25, 9, 38 - 200)
LIVE_GEOMETRY = (1.25, 9, 38)
UNFOCUSED_BOX = (19, 124, 149, 148)   # crop coordinates (window y - 200)
FOCUSED_INTERIOR = (20, 125, 148, 147)


def _image(path):
    from PIL import Image
    return Image.open(path).convert("RGB")


def _capture_of(image):
    return image.width, image.height, image.tobytes()


def _focused_crop():
    return _image(FOCUSED_LIVE).crop(CROP)


def _repaint_frame(image, interior, paint):
    """Repaint every non-interior pixel of the box frame band via paint(pixel)."""
    x0, y0, x1, y1 = interior
    pixels = image.load()
    for y in range(y0 - 5, y1 + 5):
        for x in range(x0 - 5, x1 + 5):
            if not (x0 <= x < x1 and y0 <= y < y1):
                pixels[x, y] = paint(pixels[x, y])
    return image


def _shifted(image, dx, dy):
    from PIL import Image
    moved = Image.new("RGB", image.size, BACKGROUND)
    moved.paste(image, (dx, dy))
    return moved


class LiveJoinBoxCaptureTests(unittest.TestCase):
    """The v8b failure: focus shrank the white interior and the old exact check stopped input."""

    def join_state(self, image, box=UNFOCUSED_BOX, geometry=CROP_GEOMETRY):
        capture = _capture_of(image)
        with mock.patch.object(prepare, "_capture", return_value=capture):
            return prepare._join_state(1, mock.Mock(), geometry, box)[1]

    def test_fixtures_have_the_recorded_geometry(self):
        live = _image(FOCUSED_LIVE)
        self.assertEqual(live.size, (768, 722))
        self.assertEqual(prepare.find_join_box(_capture_of(live), LIVE_GEOMETRY), (20, 325, 148, 347))
        self.assertEqual(prepare.find_join_box(_capture_of(_focused_crop()), CROP_GEOMETRY), FOCUSED_INTERIOR)
        unfocused = _capture_of(_image(UNFOCUSED_CROP))
        self.assertEqual(prepare.find_join_box(unfocused, CROP_GEOMETRY), UNFOCUSED_BOX)
        state = prepare.field_state(unfocused, UNFOCUSED_BOX, 1.25)
        self.assertEqual((state["present"], state["focused"], state["empty"]), (True, False, True))

    def test_caret_does_not_cut_the_measured_width(self):
        # The old bottom-margin row met the caret and reported x0=26.
        x0 = prepare.find_join_box(_capture_of(_focused_crop()), CROP_GEOMETRY)[0]
        self.assertEqual(x0, 20)

    def test_live_focused_capture_is_accepted_as_the_same_focused_empty_box(self):
        self.assertEqual(prepare.focus_inset(_capture_of(_focused_crop()), UNFOCUSED_BOX,
                                             FOCUSED_INTERIOR, 1.25), "focus")
        state = self.join_state(_focused_crop())
        self.assertEqual((state["present"], state["focused"], state["empty"]), (True, True, True))
        state = self.join_state(_image(FOCUSED_LIVE), (19, 324, 149, 348), LIVE_GEOMETRY)
        self.assertTrue(state["focused"])

    def test_unfocused_capture_is_unchanged_and_unfocused(self):
        state = self.join_state(_image(UNFOCUSED_CROP))
        self.assertEqual((state["focused"], state["empty"]), (False, True))

    def test_moved_box_stops_input(self):
        for dx, dy in ((0, 2), (0, 40), (1, 0), (-1, 0), (0, -1)):
            for name, source in (("unfocused", _image(UNFOCUSED_CROP)), ("focused", _focused_crop())):
                with self.subTest(dx=dx, dy=dy, source=name), \
                        self.assertRaisesRegex(prepare.HarnessFailure, "layout moved"):
                    self.join_state(_shifted(source, dx, dy))

    def test_shrunk_box_without_focus_ring_stops_input(self):
        bare = _repaint_frame(_focused_crop(), FOCUSED_INTERIOR, lambda _p: BACKGROUND)
        self.assertIsNone(prepare.join_frame(_capture_of(bare), FOCUSED_INTERIOR, 1.25))
        with self.assertRaisesRegex(prepare.HarnessFailure, "layout moved"):
            self.join_state(bare)
        partial = _focused_crop()
        pixels = partial.load()
        for y in range(118, 154):   # drop the left stroke only
            for x in range(14, 20):
                pixels[x, y] = BACKGROUND
        with self.assertRaisesRegex(prepare.HarnessFailure, "layout moved"):
            self.join_state(partial)

    def test_shrink_beyond_the_stroke_width_stops_input(self):
        deeper = (22, 127, 146, 145)   # inset 3 > the 2-pixel limit at 125%
        image = _repaint_frame(_focused_crop(), deeper, lambda _p: BLUE)
        self.assertEqual(prepare.find_join_box(_capture_of(image), CROP_GEOMETRY)[1:], deeper[1:])
        with self.assertRaisesRegex(prepare.HarnessFailure, "layout moved"):
            self.join_state(image)

    def test_hover_stroke_keeps_the_box_but_never_counts_as_focus(self):
        def gray(pixel):
            coverage = (255 - pixel[0]) / 255   # the selection stroke has red 0
            if pixel == BACKGROUND or coverage <= 0:
                return pixel
            value = round(coverage * 105 + (1 - coverage) * 255)
            return value, value, value
        hovered = _repaint_frame(_focused_crop(), FOCUSED_INTERIOR, gray)
        self.assertEqual(prepare.join_frame(_capture_of(hovered), FOCUSED_INTERIOR, 1.25), "hover")
        self.assertFalse(self.join_state(hovered)["focused"])

    def test_two_boxes_fail_before_input(self):
        image = _image(UNFOCUSED_CROP)
        image.paste(image.crop((14, 118, 156, 154)), (14, 160))
        with self.assertRaisesRegex(prepare.HarnessFailure, "not uniquely recognized"):
            prepare.find_join_box(_capture_of(image), CROP_GEOMETRY)
        with self.assertRaisesRegex(prepare.HarnessFailure, "not uniquely recognized"):
            self.join_state(image)

    def run_live_join(self, after_click, *, typed=True, confirm=True):
        """enter_join_code against the real captures: unfocused, then after_click."""
        events = {"focus_clicks": 0, "typed": [], "joins": 0}
        before = _image(UNFOCUSED_CROP)

        def capture(_handle, _adapter):
            if not events["focus_clicks"]:
                return _capture_of(before)
            image = after_click.copy()
            if events["typed"] and typed:
                pixels = image.load()
                for glyph in range(7):
                    gx = 30 + glyph * 9
                    for y in range(130, 141):
                        for x in range(gx, gx + 3):
                            pixels[x, y] = INK
            return _capture_of(image)

        def click(_pid, x, y, _adapter, settle=0):
            x0, y0, x1, y1 = UNFOCUSED_BOX
            if x0 <= x < x1 and y0 <= y < y1:
                events["focus_clicks"] += 1
            else:
                events["joins"] += 1

        adapter = mock.Mock()
        adapter.user32.VkKeyScanW.side_effect = lambda char: ord(char)
        with mock.patch.object(prepare, "_focus_signed", return_value=1), \
             mock.patch.object(prepare, "form_geometry", return_value=CROP_GEOMETRY), \
             mock.patch.object(prepare, "_capture", side_effect=capture), \
             mock.patch.object(prepare, "click_desktop", side_effect=click), \
             mock.patch.object(prepare, "type_text", side_effect=lambda value, *_a: events["typed"].append(value)), \
             mock.patch.object(prepare.time, "sleep"), \
             mock.patch.object(prepare.time, "monotonic", side_effect=iter(range(0, 100000, 1))):
            try:
                return prepare.enter_join_code(7, "ABC-234", adapter, mock.Mock(return_value=confirm)), events
            except prepare.HarnessFailure as exc:
                exc.events = events
                raise

    def test_live_focus_capture_lets_the_code_be_typed_and_joined(self):
        receipt, events = self.run_live_join(_focused_crop())
        self.assertEqual((events["focus_clicks"], events["typed"], events["joins"]), (1, ["ABC-234"], 1))
        self.assertEqual(receipt["box"], list(UNFOCUSED_BOX))

    def test_live_focus_but_code_not_visible_never_presses_join(self):
        with self.assertRaisesRegex(prepare.HarnessFailure, "not visibly entered") as caught:
            self.run_live_join(_focused_crop(), typed=False)
        self.assertEqual(caught.exception.events["joins"], 0)

    def test_live_focus_without_server_group_fails(self):
        with self.assertRaisesRegex(prepare.HarnessFailure, "did not report the paired group"):
            self.run_live_join(_focused_crop(), confirm=False)

    def test_ring_less_shrink_after_click_types_nothing(self):
        bare = _repaint_frame(_focused_crop(), FOCUSED_INTERIOR, lambda _p: BACKGROUND)
        with self.assertRaisesRegex(prepare.HarnessFailure, "layout moved") as caught:
            self.run_live_join(bare)
        self.assertEqual(caught.exception.events["typed"], [])


# Real capture of the same desktop (v8b run b50004e9..., outbound-join-failed-b.png)
# right after the code was typed: focused box, caret at x 87..90. The typed
# pairing code's pixels (x 20..86 of the interior) are painted white and the
# caret's left anti-aliased column copied from its right one; tests overlay
# synthetic glyphs where the code was.
TYPED_MASKED = FIXTURES / "desktop-join-box-typed-masked-live-125pct.png"
TYPED_MASKED_SHA256 = "204ef15648e80d5a3a907305d6595d39c439ded37479f6b228537afd6eea2998"
LIVE_BOX = (19, 324, 149, 348)          # unfocused interior, window pixels
LIVE_FOCUSED = (20, 325, 148, 347)
CARET = (87, 91)                        # caret columns, window pixels
OLD_FIXED_BAND = (9 + round(62 * 1.25), 9 + round(92 * 1.25))   # pre-fix probe columns


def _with_code(image, dy=0):
    """Overlay seven synthetic glyph runs left of the caret (no real code)."""
    image = image.copy()
    pixels = image.load()
    for glyph in range(7):
        gx = 26 + glyph * 9
        for y in range(331 + dy, 342 + dy):
            for x in range(gx, gx + 3):
                pixels[x, y] = INK
    return image


class LiveTypedJoinBoxTests(unittest.TestCase):
    """The v8b b50004e9 failure: the typed code and caret reached the fixed probe columns."""

    def typed(self):
        return _image(TYPED_MASKED)

    def join_state(self, image, **kwargs):
        with mock.patch.object(prepare, "_capture", return_value=_capture_of(image)):
            return prepare._join_state(1, mock.Mock(), LIVE_GEOMETRY, LIVE_BOX, **kwargs)[1]

    def test_fixture_is_masked_and_holds_no_code(self):
        import hashlib
        self.assertEqual(hashlib.sha256(TYPED_MASKED.read_bytes()).hexdigest(), TYPED_MASKED_SHA256)
        image = self.typed()
        self.assertEqual(image.size, (768, 722))
        pixels = image.load()
        x0, y0, x1, y1 = LIVE_FOCUSED
        ink = {(x, y) for y in range(y0, y1) for x in range(x0, x1) if pixels[x, y] != WHITE}
        self.assertTrue(ink)
        self.assertTrue(all(CARET[0] <= x < CARET[1] for x, _y in ink))
        for x in range(*CARET):   # a caret is a uniform vertical bar (+-1 capture noise)
            column = [pixels[x, y] for y in range(y0 + 1, y1 - 1)]
            for channel in range(3):
                self.assertLessEqual(max(p[channel] for p in column) - min(p[channel] for p in column), 1)
        state = prepare.field_state(_capture_of(image), LIVE_BOX, 1.25)
        self.assertTrue(state["empty"])

    def test_fixed_probe_columns_reproduce_the_live_failure(self):
        capture = _capture_of(self.typed())
        with self.assertRaisesRegex(prepare.HarnessFailure, "not uniquely recognized"):
            prepare.find_join_box(capture, LIVE_GEOMETRY, OLD_FIXED_BAND)

    def test_scan_bands_stay_right_of_hint_code_and_caret(self):
        hint = _capture_of(_image(FOCUSED_LIVE))
        typed = _capture_of(_with_code(self.typed()))
        first = 9 + round(prepare.JOIN_PROBE_COLUMNS[0] * 1.25)
        derived = prepare.join_band(LIVE_BOX, 1.25)
        self.assertGreaterEqual(first, CARET[1] + 5)
        self.assertGreaterEqual(derived[0], CARET[1] + 5)
        self.assertLess(derived[1], LIVE_FOCUSED[2] - 1)
        for capture in (hint, typed):
            self.assertEqual(prepare.find_join_box(capture, LIVE_GEOMETRY), LIVE_FOCUSED)
            self.assertEqual(prepare.find_join_box(capture, LIVE_GEOMETRY, derived), LIVE_FOCUSED)

    def test_typed_code_is_recognized_in_the_focused_box_at_125pct(self):
        state = self.join_state(_with_code(self.typed()), require_focus=True)
        self.assertTrue(state["focused"])
        self.assertFalse(state["empty"])
        self.assertGreaterEqual(state["runs"], 5)

    def test_missing_code_reads_as_empty(self):
        state = self.join_state(self.typed(), require_focus=True)
        self.assertTrue(state["empty"])

    def test_box_moved_after_typing_stops(self):
        for dx, dy in ((0, 2), (0, 40), (0, -1), (1, 0), (-1, 0), (-30, 0)):
            with self.subTest(dx=dx, dy=dy), \
                    self.assertRaisesRegex(prepare.HarnessFailure, "layout moved|not uniquely recognized"):
                self.join_state(_shifted(_with_code(self.typed()), dx, dy), require_focus=True)

    def test_border_missing_after_typing_stops(self):
        bare = _repaint_frame(_with_code(self.typed()), LIVE_FOCUSED, lambda _p: BACKGROUND)
        with self.assertRaisesRegex(prepare.HarnessFailure, "layout moved"):
            self.join_state(bare, require_focus=True)
        # Interior restored to the unfocused rectangle without any stroke: the
        # same box, but the keys may have gone elsewhere.
        unstroked = _repaint_frame(_with_code(self.typed()), LIVE_FOCUSED,
                                   lambda _p: BACKGROUND)
        pixels = unstroked.load()
        x0, y0, x1, y1 = LIVE_BOX
        for y in range(y0, y1):
            for x in range(x0, x1):
                if not (LIVE_FOCUSED[0] <= x < LIVE_FOCUSED[2] and LIVE_FOCUSED[1] <= y < LIVE_FOCUSED[3]):
                    pixels[x, y] = WHITE
        with self.assertRaisesRegex(prepare.HarnessFailure, "lost focus"):
            self.join_state(unstroked, require_focus=True)

    def test_duplicate_or_clipped_box_after_typing_stops(self):
        image = _with_code(self.typed())
        image.paste(image.crop((14, 318, 156, 354)), (14, 400))
        with self.assertRaisesRegex(prepare.HarnessFailure, "not uniquely recognized"):
            self.join_state(image, require_focus=True)
        clipped = _with_code(self.typed()).crop((0, 0, 768, 340))
        with self.assertRaisesRegex(prepare.HarnessFailure, "clipped"):
            self.join_state(clipped, require_focus=True)

    def run_live_join(self, after_typing, *, confirm=True):
        """enter_join_code on real captures: unfocused, focused empty, then after_typing."""
        events = {"focus_clicks": 0, "typed": [], "joins": 0}
        before = _image(UNFOCUSED_CROP)

        def capture(_handle, _adapter):
            if events["typed"]:
                image = after_typing
            elif events["focus_clicks"]:
                image = _focused_crop()
            else:
                image = before
            return _capture_of(image)

        def click(_pid, x, y, _adapter, settle=0):
            x0, y0, x1, y1 = UNFOCUSED_BOX
            if x0 <= x < x1 and y0 <= y < y1:
                events["focus_clicks"] += 1
            else:
                events["joins"] += 1

        adapter = mock.Mock()
        adapter.user32.VkKeyScanW.side_effect = lambda char: ord(char)
        with mock.patch.object(prepare, "_focus_signed", return_value=1), \
             mock.patch.object(prepare, "form_geometry", return_value=CROP_GEOMETRY), \
             mock.patch.object(prepare, "_capture", side_effect=capture), \
             mock.patch.object(prepare, "click_desktop", side_effect=click), \
             mock.patch.object(prepare, "type_text", side_effect=lambda value, *_a: events["typed"].append(value)), \
             mock.patch.object(prepare.time, "sleep"), \
             mock.patch.object(prepare.time, "monotonic", side_effect=iter(range(0, 100000, 1))):
            try:
                return prepare.enter_join_code(7, "ABC-234", adapter, mock.Mock(return_value=confirm)), events
            except prepare.HarnessFailure as exc:
                exc.events = events
                raise

    def typed_crop(self, image):
        return image.crop(CROP)

    def test_end_to_end_typed_capture_presses_join(self):
        receipt, events = self.run_live_join(self.typed_crop(_with_code(self.typed())))
        self.assertEqual((events["focus_clicks"], events["typed"], events["joins"]), (1, ["ABC-234"], 1))
        self.assertEqual(receipt["box"], list(UNFOCUSED_BOX))

    def test_end_to_end_missing_code_never_presses_join(self):
        with self.assertRaisesRegex(prepare.HarnessFailure, "not visibly entered") as caught:
            self.run_live_join(self.typed_crop(self.typed()))
        self.assertEqual(caught.exception.events["joins"], 0)

    def test_end_to_end_moved_or_borderless_box_never_presses_join(self):
        moved = self.typed_crop(_shifted(_with_code(self.typed()), 0, 40))
        bare = self.typed_crop(_repaint_frame(_with_code(self.typed()), LIVE_FOCUSED, lambda _p: BACKGROUND))
        for name, image in (("moved", moved), ("borderless", bare)):
            with self.subTest(name), self.assertRaisesRegex(prepare.HarnessFailure, "layout moved") as caught:
                self.run_live_join(image)
            self.assertEqual(caught.exception.events["joins"], 0)

    def test_end_to_end_without_server_group_fails(self):
        with self.assertRaisesRegex(prepare.HarnessFailure, "did not report the paired group"):
            self.run_live_join(self.typed_crop(_with_code(self.typed())), confirm=False)


if __name__ == "__main__":
    unittest.main()
