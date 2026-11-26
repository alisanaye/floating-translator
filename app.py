# -*- coding: utf-8 -*-
"""مترجم شناور — floating selection translator for Windows.

هر متنی را در هر برنامه‌ای (از جمله PDF) انتخاب کنید، ترجمه‌اش فوراً در یک
پنجره‌ی کوچکِ همیشه-رو نمایش داده می‌شود. پنجره خودبه‌خود جابه‌جا نمی‌شود؛
فقط با کشیدن نوار عنوان توسط شما حرکت می‌کند و موقعیتش ذخیره می‌شود.
کپی ترجمه: دکمه‌ی «کپی» در نوار عنوان / راست‌کلیک → «کپی ترجمه» / Ctrl+Shift+C.

اجرا:  pythonw app.py     (یا دوبار کلیک روی run.bat)
تست:   python app.py --selftest

فقط کتابخانه‌ی استاندارد پایتون؛ هیچ نصب اضافه‌ای لازم نیست.
"""
import ctypes
import hashlib
import html
import json
import math
import os
import queue
import sys
import threading
import time
import tkinter as tk
import tkinter.font as tkfont
import tkinter.ttk as ttk
import unicodedata
import urllib.error
import urllib.parse
import urllib.request
from copy import deepcopy
from ctypes import wintypes
import xml.etree.ElementTree as ET

try:
    import lucide
    import tksvg
    HAS_LUCIDE = True
except ImportError:
    HAS_LUCIDE = False

APP_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_PATH = os.path.join(APP_DIR, "config.json")
USAGE_PATH = os.path.join(APP_DIR, "usage.json")
USAGE_LOCK = threading.Lock()

WIDTH, HEIGHT = 420, 240
MAX_CHARS = 3000
TRIGGER_DRAG_PX = 8          # اینچقدر جابه‌جایی ماوس = انتخاب متن
DOUBLE_CLICK_S = 0.45        # دو کلیک سریع = انتخاب یک کلمه
DOUBLE_CLICK_PX = 6
COPY_SETTLE_MS = 180         # فرصت به برنامه‌ی مبدأ تا Ctrl+C را انجام دهد
POLL_MS = 40                 # حلقه‌ی رویدادهای رابط کاربری
MOUSE_POLL_MS = 15           # پایش دکمه‌ی چپ ماوس

RTL_LANGS = {"fa", "ar", "he", "ur"}
DEBUG = "--debug" in sys.argv

# مدلهای قابل انتخاب: (نوع API، برچسب کوتاه نوار عنوان، نام کامل)
# سهمیهی روزانهی هر مدل جداست؛ وقتی یکی پر شد، فقط مدل دیگری انتخاب کنید.
# فهرست با کلید خودِ کاربر تست شده (۲۰۲۶-۱۱)؛ مدلهای سهمیهصفر (Gemini 4 Argon/Carbon/Neon،
# 3.1 Pro، 2.5 Pro، Omni، Pro/Flash Latest) عمداً حذف شدهاند چون با کلید رایگان 429 میدهند.
# مدلهای Lite سهمیهی روزانهی بیشتری دارند (۵۰۰ در روز در برابر ۲۰).
MODELS = {
    "gemini-4-hydrogen":        ("gemini", "4 Hydrogen",       "Gemini 4 Hydrogen"),
    "gemini-3.8-flash":         ("gemini", "3.8 Flash",        "Gemini 3.8 Flash"),
    "gemini-3.7-flash":         ("gemini", "3.7 Flash",        "Gemini 3.7 Flash"),
    "gemini-3.6-flash":         ("gemini", "3.6 Flash",        "Gemini 3.6 Flash"),
    "gemini-3.5-flash":         ("gemini", "3.5 Flash",        "Gemini 3.5 Flash"),
    "gemini-3.5-flash-lite":    ("gemini", "3.5 Flash Lite",   "Gemini 3.5 Flash Lite"),
    "gemini-3.1-flash-lite":    ("gemini", "3.1 Flash Lite",   "Gemini 3.1 Flash Lite"),
    "gemini-3-flash-preview":   ("gemini", "3 Flash",          "Gemini 3 Flash (preview)"),
    "gemini-2.5-flash":         ("gemini", "2.5 Flash",        "Gemini 2.5 Flash"),
    "gemini-2.5-flash-lite":    ("gemini", "2.5 Flash Lite",   "Gemini 2.5 Flash Lite"),
    "google-translate":         ("google", "Translate v2",     "Cloud Translation v2"),
}
DEFAULT_MODEL = "gemini-4-hydrogen"
LABEL_OF = {label: mid for mid, (_e, _s, label) in MODELS.items()}
RETRY_HTTP = {408, 429, 500, 502, 503, 504}     # این خطاها گذرا هستند و تکرار میشوند
# (RPM, TPM ورودی, RPD) فقط حدهای مشاهدهشده در AI Studio؛ None یعنی نامشخص.
LIMITS = {
    "gemini-3.5-flash-lite": (15, 250000, 500),
    "gemini-3.1-flash-lite": (15, 250000, 500),
    "gemini-3-flash-preview": (5, 250000, 20),
    "gemini-2.5-flash": (5, 250000, 20),
    "gemini-2.5-flash-lite": (10, 250000, 20),
}


def log(msg):
    """با سوییچ --debug، هر رویداد یک خط در debug.log ثبت می‌شود."""
    if not DEBUG:
        return
    try:
        with open(os.path.join(APP_DIR, "debug.log"), "a", encoding="utf-8") as f:
            f.write("%s %s\n" % (time.strftime("%H:%M:%S"), msg))
    except OSError:
        pass

DEFAULTS = {
    "model": DEFAULT_MODEL,
    "api_key": "",
    "api_keys": [],
    "target": "fa",
    "proxy": "",              # مثلاً http://127.0.0.1:10809
    "timeout": 20,            # مهلت هر تلاش (ثانیه)
    "retries": 2,             # تعداد تلاشهای مجدد در خطای گذرا
    "rtl": None,              # None = خودکار بر اساس زبان مقصد | True/False = دستی
    "autocopy": False,        # هر ترجمه بلافاصله در کلیپ‌بورد کپی شود
    "watch": True,            # انتخاب متن در هر برنامه‌ای خودکار ترجمه شود
    "opacity": 1.0,
    "x": 60,
    "y": 60,
    "width": WIDTH,
    "height": HEIGHT,
    "theme": "dark",
    "locked": False,
}


def _int(value, default, lo, hi):
    try:
        return max(lo, min(hi, int(value)))
    except (TypeError, ValueError):
        return default


# ---------------------------------------------------------------- config
def load_config(path=CONFIG_PATH):
    cfg = dict(DEFAULTS)
    raw = {}
    try:
        with open(path, "r", encoding="utf-8") as f:
            raw = json.load(f)
        if isinstance(raw, dict):
            cfg.update(raw)
    except (OSError, ValueError):
        pass
    if "model" not in raw:                      # config قدیمی: engine/gemini_model
        legacy = cfg.get("gemini_model")
        if cfg.get("engine") == "google":
            cfg["model"] = "google-translate"
        elif legacy in MODELS:
            cfg["model"] = legacy
        else:
            cfg["model"] = DEFAULT_MODEL
    if cfg.get("model") not in MODELS:          # نام ناشناخته در config
        cfg["model"] = DEFAULT_MODEL
    cfg.pop("engine", None)
    cfg.pop("gemini_model", None)
    cfg["timeout"] = _int(cfg.get("timeout"), DEFAULTS["timeout"], 5, 120)
    cfg["retries"] = _int(cfg.get("retries"), DEFAULTS["retries"], 0, 5)
    if cfg.get("theme") not in ("dark", "light"):
        cfg["theme"] = "dark"
    keys = cfg.get("api_keys")
    keys = [k.strip() for k in keys if isinstance(k, str) and k.strip()] if isinstance(keys, list) else []
    active = str(cfg.get("api_key") or "").strip()
    if active and active not in keys:
        keys.insert(0, active)                 # مهاجرت خودکار config تک‌کلیدی قدیمی
    cfg["api_keys"] = list(dict.fromkeys(keys))
    cfg["api_key"] = active or (cfg["api_keys"][0] if cfg["api_keys"] else "")
    return cfg


def save_config(cfg, path=CONFIG_PATH):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(cfg, f, ensure_ascii=False, indent=2)


def key_id(key):
    return hashlib.sha256(key.encode("utf-8")).hexdigest()[:12]


def key_label(key):
    return "…" + key[-6:] if key else "بدون کلید"


def _read_usage():
    try:
        with open(USAGE_PATH, "r", encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def record_usage(key, model, metadata=None, now=None):
    """یک پاسخ موفق را ذخیره می‌کند؛ خطای دیسک نباید ترجمه را خراب کند."""
    if not key:
        return
    metadata = metadata or {}
    now = time.time() if now is None else now
    event = [now, _int(metadata.get("promptTokenCount"), 0, 0, 10**9),
             _int(metadata.get("candidatesTokenCount"), 0, 0, 10**9)]
    try:
        with USAGE_LOCK:
            data = _read_usage()
            bucket = data.get(key_id(key))
            if not isinstance(bucket, dict):
                bucket = data[key_id(key)] = {}
            events = bucket.get(model)
            if not isinstance(events, list):
                events = []
            events = [e for e in events if isinstance(e, list) and len(e) >= 3
                      and isinstance(e[0], (int, float))]
            events.append(event)
            bucket[model] = [e for e in events if e[0] >= now - 86400]
            tmp = USAGE_PATH + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(data, f, separators=(",", ":"))
            os.replace(tmp, USAGE_PATH)
    except (OSError, TypeError, ValueError) as exc:
        log("usage save: %r" % (exc,))


def usage_rows(keys, now=None):
    """(کلید، مدل، مصرف دقیقه، توکن دقیقه، مصرف ۲۴ساعت، حدها) برای نمایش."""
    now = time.time() if now is None else now
    data = _read_usage()
    rows = []
    for key in keys:
        bucket = data.get(key_id(key))
        bucket = bucket if isinstance(bucket, dict) else {}
        for model in MODELS:
            events = bucket.get(model)
            events = [e for e in events if isinstance(e, list) and len(e) >= 3
                      and isinstance(e[0], (int, float))] if isinstance(events, list) else []
            minute = [e for e in events if now - e[0] < 60]
            day = [e for e in events if now - e[0] < 86400]
            rows.append((key, model, len(minute), sum(e[1] for e in minute),
                         len(day), LIMITS.get(model, (None, None, None))))
    return rows


# ---------------------------------------------------------------- translation
def parse_google(payload):
    """(detected_source_language, translated_text) از پاسخ Cloud Translation v2."""
    t = payload["data"]["translations"][0]
    return t.get("detectedSourceLanguage", ""), html.unescape(t["translatedText"])


def _proxy_url(cfg):
    """پروکسی را کامل میکند: 127.0.0.1:10809 → http://127.0.0.1:10809"""
    proxy = (cfg.get("proxy") or "").strip().rstrip("/")
    if proxy and "://" not in proxy:
        proxy = "http://" + proxy
    return proxy


def _post_json(url, body, cfg):
    """POST با مهلت مشخص و چند تلاش مجدد (backoff) برای خطاهای گذرا."""
    data = json.dumps(body).encode("utf-8")
    proxy = _proxy_url(cfg)
    opener = (urllib.request.build_opener(
        urllib.request.ProxyHandler({"http": proxy, "https": proxy})) if proxy else None)
    timeout = _int(cfg.get("timeout"), DEFAULTS["timeout"], 5, 120)
    attempts = _int(cfg.get("retries"), DEFAULTS["retries"], 0, 5) + 1
    for attempt in range(attempts):
        req = urllib.request.Request(url, data=data,
                                     headers={"Content-Type": "application/json"})
        try:
            resp = (opener.open if opener else urllib.request.urlopen)(req, timeout=timeout)
            with resp:
                return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as exc:
            if attempt == attempts - 1 or exc.code not in RETRY_HTTP:
                raise
            log("retry %d/%d after HTTP %d" % (attempt + 1, attempts - 1, exc.code))
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            if attempt == attempts - 1:
                raise
            log("retry %d/%d after %r" % (attempt + 1, attempts - 1, exc))
        time.sleep(0.5 * (attempt + 1))


def _fail(exc, cfg=None):
    """خطای شبکه/API را به یک جمله‌ی قابل‌فهم تبدیل می‌کند."""
    cfg = cfg or {}
    proxy = _proxy_url(cfg)
    if isinstance(exc, urllib.error.HTTPError):
        try:
            raw = exc.read().decode("utf-8", "replace")
        except Exception:
            raw = ""
        try:                                    # خطاهای واقعی API جیسون دارند
            detail = json.loads(raw)["error"]["message"]
        except Exception:
            detail = ""
        if exc.code == 429:
            hint = "سهمیه‌ی این مدل پر شده (429) — از فهرست مدل بالای پنل مدل دیگری انتخاب کنید."
            detail = hint + (" " + detail if detail else "")
        elif not detail:
            if exc.code in (401, 403):
                detail = ("گوگل دسترسی این شبکه را رد کرد (%d). معمولاً یعنی IP مسدود است — "
                          "VPN یا پروکسی روشن کنید و در تنظیمات مقدار پروکسی را بدهید." % exc.code)
            elif exc.code == 400:
                detail = "درخواست رد شد (400) — کلید API و زبان مقصد را چک کنید."
            else:
                detail = "گوگل خطای HTTP %d برگرداند." % exc.code
    elif isinstance(exc, (urllib.error.URLError, TimeoutError, OSError)):
        reason = getattr(exc, "reason", exc)
        if isinstance(reason, TimeoutError) or "timed out" in str(reason).lower():
            detail = ("زمان پاسخ تمام شد (%d ثانیه × %d تلاش) — شبکه یا پروکسی کند است؛ "
                      "در تنظیمات «مهلت پاسخ» را بیشتر کنید"
                      % (_int(cfg.get("timeout"), DEFAULTS["timeout"], 5, 120),
                         _int(cfg.get("retries"), DEFAULTS["retries"], 0, 5) + 1))
            detail += (" یا پروکسی دیگری بدهید." if proxy else ".")
        elif proxy:
            detail = ("اتصال از طریق پروکسی %s برقرار نشد (%s) — کلاینت VPN روشن و متصل است؟"
                      % (proxy, reason))
        else:
            detail = ("اتصال به گوگل برقرار نشد (%s) — اینترنت یا پروکسی را چک کنید."
                      % (reason,))
    else:
        detail = str(exc)
    return RuntimeError("خطای ترجمه: " + " ".join(str(detail).split())[:280])


def _google(text, key, target, cfg):
    url = "https://translation.googleapis.com/language/translate/v2?key=" + urllib.parse.quote(key)
    payload = _post_json(url, {"q": [text], "target": target, "format": "text"}, cfg)
    record_usage(key, "google-translate")
    return parse_google(payload)


def _gemini(text, cfg, target, model):
    key = cfg["api_key"].strip()
    url = ("https://generativelanguage.googleapis.com/v1beta/models/"
           + urllib.parse.quote(model) + ":generateContent?key="
           + urllib.parse.quote(key))
    prompt = ("Translate the text below into ISO language code '%s'. "
              "If it is already in that language, translate it into English instead. "
              "Reply with ONLY the translation - no notes, no quotes.\n\n%s" % (target, text))
    payload = _post_json(url, {"contents": [{"parts": [{"text": prompt}]}]}, cfg)
    record_usage(key, model, payload.get("usageMetadata"))
    try:
        out = payload["candidates"][0]["content"]["parts"][0]["text"].strip()
    except (KeyError, IndexError):
        raise RuntimeError("پاسخ نامعتبر از Gemini")
    return "", out


def translate(text, cfg):
    key = (cfg.get("api_key") or "").strip()
    if not key:
        raise RuntimeError("کلید API تنظیم نشده — راست‌کلیک → تنظیمات")
    target = (cfg.get("target") or "fa").strip().lower() or "fa"
    model = cfg.get("model") if cfg.get("model") in MODELS else DEFAULT_MODEL
    try:
        if MODELS[model][0] == "gemini":
            return _gemini(text, cfg, target, model)
        src, out = _google(text, key, target, cfg)
        if src and src.split("-")[0] == target.split("-")[0]:
            # متن از قبل به زبان مقصد بوده؛ برعکس ترجمه کن
            src, out = _google(text, key, "en" if not target.startswith("en") else "fa", cfg)
        return src, out
    except Exception as exc:            # noqa: BLE001 - همه به پیام کاربر تبدیل می‌شود
        raise _fail(exc, cfg)


# ---------------------------------------------------------------- selection gesture
def is_select_gesture(pt, down, last_pt, last_up, now):
    """آیا این رهاکردن دکمه‌ی چپ، یک انتخاب متن است؟ (کشیدن یا دوبار کلیک)"""
    if down is None:
        return False
    if max(abs(pt[0] - down[0]), abs(pt[1] - down[1])) >= TRIGGER_DRAG_PX:
        return True
    return bool(last_up and last_pt
                and now - last_up < DOUBLE_CLICK_S
                and max(abs(pt[0] - last_pt[0]), abs(pt[1] - last_pt[1])) <= DOUBLE_CLICK_PX)


# ---------------------------------------------------------------- mouse input
# عمداً از هوک سطح‌پایین (WH_MOUSE_LL) استفاده نمی‌کنیم: در پایتون، کال‌بکِ هوک
# باید GIL بگیرد و اگر دیر برسد ویندوز بی‌صدا حذفش می‌کند. پولینگِ ۲۵ میلی‌ثانیه‌ای
# وضعیت دکمه‌ی چپ همین کار را می‌کند، بدون هوک و بدون ترد دوم.
user32 = ctypes.WinDLL("user32", use_last_error=True)

VK_LBUTTON = 0x01
VK_CONTROL, VK_C, KEYEVENTF_KEYUP = 0x11, 0x43, 0x0002
OUR_PID = os.getpid()

user32.GetAsyncKeyState.argtypes = [ctypes.c_int]
user32.GetAsyncKeyState.restype = ctypes.c_short
user32.GetCursorPos.argtypes = [ctypes.POINTER(wintypes.POINT)]
user32.WindowFromPoint.argtypes = [wintypes.POINT]
user32.WindowFromPoint.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.keybd_event.argtypes = [wintypes.BYTE, wintypes.BYTE, wintypes.DWORD, ctypes.c_void_p]
user32.GetClipboardSequenceNumber.restype = wintypes.DWORD
user32.GetClipboardData.restype = wintypes.HANDLE
user32.OpenClipboard.argtypes = [wintypes.HWND]

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.GlobalLock.restype = ctypes.c_void_p
kernel32.GlobalLock.argtypes = [wintypes.HANDLE]
kernel32.GlobalUnlock.argtypes = [wintypes.HANDLE]
kernel32.GlobalAlloc.restype = wintypes.HANDLE
kernel32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
kernel32.GlobalFree.argtypes = [wintypes.HANDLE]
kernel32.GlobalFree.restype = wintypes.HANDLE
CF_UNICODETEXT = 13
GMEM_MOVEABLE = 0x0002
user32.EmptyClipboard.restype = wintypes.BOOL
user32.SetClipboardData.argtypes = [wintypes.UINT, wintypes.HANDLE]
user32.SetClipboardData.restype = wintypes.HANDLE


def set_clipboard(text):
    """متن را در کلیپ‌بورد می‌گذارد — مستقیم از وین‌۳۲، بدون دخالت Tk.

    این تابع هرگز `root.clipboard_clear()` را فرا نمی‌کند: Tk پس از آن کلیپ‌بورد را
    قفل‌شده نگه می‌دارد و `Ctrl+C` برنامه‌های دیگر هیچ اثری ندارد.
    """
    if not text:
        return False
    data = text.encode("utf-16-le") + b"\x00\x00"
    handle = kernel32.GlobalAlloc(GMEM_MOVEABLE, len(data))
    if not handle:
        return False
    ptr = kernel32.GlobalLock(handle)
    if not ptr:
        kernel32.GlobalFree(handle)
        return False
    try:
        ctypes.memmove(ptr, data, len(data))
    finally:
        kernel32.GlobalUnlock(handle)
    if not user32.OpenClipboard(None):
        kernel32.GlobalFree(handle)
        return False
    try:
        user32.EmptyClipboard()
        if not user32.SetClipboardData(CF_UNICODETEXT, handle):
            kernel32.GlobalFree(handle)
            return False
        return True
    finally:
        user32.CloseClipboard()


def clipboard_text():
    """متن کلیپ‌بورد را مستقیم از وین۳۲ می‌خواند، بدون درگیر کردن Tk.

    نکته‌ی مهم: هرگز نباید root.clipboard_clear() صدا زده شود — Tk بعد از آن
    کلیپ‌بورد را قفل‌شده نگه می‌دارد و Ctrl+C برنامه‌ی دیگر هیچ اثری ندارد.
    """
    if not user32.OpenClipboard(None):
        return ""
    try:
        handle = user32.GetClipboardData(CF_UNICODETEXT)
        if not handle:
            return ""
        ptr = kernel32.GlobalLock(handle)
        if not ptr:
            return ""
        try:
            return ctypes.wstring_at(ptr)
        finally:
            kernel32.GlobalUnlock(handle)
    finally:
        user32.CloseClipboard()


def cursor_pos():
    p = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(p))
    return p.x, p.y


def is_our_window(pt):
    hwnd = user32.WindowFromPoint(wintypes.POINT(pt[0], pt[1]))
    if not hwnd:
        return False
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value == OUR_PID


def send_ctrl_c():
    for vk, up in ((VK_CONTROL, 0), (VK_C, 0), (VK_C, KEYEVENTF_KEYUP), (VK_CONTROL, KEYEVENTF_KEYUP)):
        user32.keybd_event(vk, 0, up, None)


class MousePoller:
    """وضعیت دکمه‌ی چپ ماوس را می‌پاید و در پایانِ هر انتخاب، on_select را صدا می‌زند."""

    def __init__(self, on_select):
        self.on_select = on_select
        self.down = None
        self.last_pt = None
        self.last_up = 0.0
        self.armed = False
        self.enabled = True                         # با راستکلیک خاموش/روشن میشود

    def poll(self):
        self.feed(cursor_pos(), bool(user32.GetAsyncKeyState(VK_LBUTTON) & 0x8000))

    def feed(self, pt, pressed, now=None):
        """یک نمونه‌ی ورودی (جدا شده تا بشود بدون ماوس واقعی تستش کرد)."""
        now = time.time() if now is None else now
        if pressed and self.down is None:
            self.down = pt
            self.armed = not is_our_window(pt)      # کشیدن از روی خودِ پنل، انتخاب نیست
        elif not pressed and self.down is not None:
            down, self.down = self.down, None
            if (self.enabled and self.armed and not is_our_window(pt)
                    and is_select_gesture(pt, down, self.last_pt, self.last_up, now)):
                self.on_select()
            self.last_pt, self.last_up = pt, now


# ---------------------------------------------------------------- overlay UI
RTL_TYPES = {"R", "AL"}


def _word_kind(word):
    """rtl یا ltr؛ برای تشخیص مرز اجراهای BiDi."""
    return "rtl" if any(unicodedata.bidirectional(ch) in RTL_TYPES
                        for ch in word) else "ltr"


def _is_rtl_text(text):
    """تشخیص جهت غالب متن مبدأ بر اساس نخستین کاراکتر جهت‌دار."""
    for ch in text:
        bidi = unicodedata.bidirectional(ch)
        if bidi in RTL_TYPES:
            return True
        if bidi == "L":
            return False
    return False


def _visual_line(units, rtl):
    """ترتیب اجراها را برای نمایش می‌چیند؛ کلمات داخل هر اجرا منطقی می‌مانند.

    Tk خودش کلمات هر اجرای RTL را برعکس می‌چیند، ولی ترتیب اجراها را نه.
    shortcut: run-level bidi، نه UBA کامل؛ توکن‌های بسیار بلند شکسته نمی‌شوند.
    """
    runs = []
    for unit in units:
        kind = "rtl" if _word_kind(unit) == "rtl" else "ltr"
        if runs and runs[-1][0] == kind:
            runs[-1][1].append(unit)
        else:
            runs.append((kind, [unit]))
    if rtl:
        runs.reverse()
    return " ".join(" ".join(parts) for _kind, parts in runs)


def _visual_layout(text, font, width, rtl):
    """شکستن خط و ترتیب بصری؛ متن منطقی برای کپی برگردانده می‌شود.

    Tk 8.6 چیدمان پاراگراف RTL ندارد و RLM/LRM را برای جهت خط نادیده می‌گیرد.
    """
    shown, source, space = [], [], font.measure(" ")
    for para in str(text).split("\n"):
        line, used = [], 0
        for word in para.split():
            need = font.measure(word)
            if line and used + space + need > width:
                shown.append(_visual_line(line, rtl))
                source.append(" ".join(line))
                line, used = [], 0
            used += (space if line else 0) + need
            line.append(word)
        shown.append(_visual_line(line, rtl))
        source.append(" ".join(line))
    return "\n".join(shown), source


THEMES = {
    "dark": {
        "bg": "#14161a",
        "bar_bg": "#1d2128",
        "divider": "#232832",
        "card_bg": "#1b1f26",
        "card_border": "#252b35",
        "hover_bg": "#282e38",
        "btn_bg": "#232833",
        "border": "#2c323d",
        "fg": "#eef1f6",
        "fg_muted": "#8b94a5",
        "fg_dim": "#5f6875",
        "accent": "#5fa8e8",
        "accent_hover": "#78b9f0",
        "accent_fg": "#14161a",
        "accent_green": "#6bd17b",
        "accent_amber": "#e0a84a",
        "accent_red": "#e06c75",
        "err_fg": "#ff9a8a",
        "key_fg": "#b39ddb",
        "select_bg": "#3b6ea5",
        "switch_off": "#363d4a",
        "knob": "#eef1f6",
        "close_hover": "#7a2c2c",
    },
    "light": {
        "bg": "#f5f7fa",
        "bar_bg": "#e9edf3",
        "divider": "#d5dce6",
        "card_bg": "#ffffff",
        "card_border": "#d8dfe9",
        "hover_bg": "#dce3ee",
        "btn_bg": "#e4e9f2",
        "border": "#c5cedb",
        "fg": "#1a1f29",
        "fg_muted": "#556070",
        "fg_dim": "#7b8696",
        "accent": "#2563eb",
        "accent_hover": "#3b82f6",
        "accent_fg": "#ffffff",
        "accent_green": "#16a34a",
        "accent_amber": "#d97706",
        "accent_red": "#dc2626",
        "err_fg": "#dc2626",
        "key_fg": "#7c3aed",
        "select_bg": "#bfdbfe",
        "switch_off": "#cbd5e1",
        "knob": "#ffffff",
        "close_hover": "#dc2626",
    },
}


def _bind_hover(widget, theme_fn, normal_bg_key, hover_bg_key,
                normal_fg_key=None, hover_fg_key=None):
    def _val(key):
        t = theme_fn() if callable(theme_fn) else theme_fn
        return t[key] if key in t else key

    def _enter(_e):
        widget.configure(bg=_val(hover_bg_key))
        if hover_fg_key is not None:
            widget.configure(fg=_val(hover_fg_key))

    def _leave(_e):
        widget.configure(bg=_val(normal_bg_key))
        if normal_fg_key is not None:
            widget.configure(fg=_val(normal_fg_key))

    widget.bind("<Enter>", _enter, add="+")
    widget.bind("<Leave>", _leave, add="+")


def _style_menu(menu, theme):
    menu.configure(bg=theme["card_bg"], fg=theme["fg"],
                   activebackground=theme["hover_bg"],
                   activeforeground=theme["fg"],
                   selectcolor=theme["accent"], bd=0, relief="flat")


def _themed_entry(parent, theme, textvariable, width=28, show=""):
    return tk.Entry(parent, textvariable=textvariable, width=width, show=show,
                    bg=theme["card_bg"], fg=theme["fg"], insertbackground=theme["fg"],
                    relief="flat", bd=0, highlightthickness=1,
                    highlightbackground=theme["border"],
                    highlightcolor=theme["accent"], font=("Segoe UI", 9))


def _center_over(win, parent):
    win.update_idletasks()
    pw, ph = parent.winfo_width(), parent.winfo_height()
    px, py = parent.winfo_rootx(), parent.winfo_rooty()
    ww, wh = win.winfo_reqwidth(), win.winfo_reqheight()
    sw, sh = win.winfo_screenwidth(), win.winfo_screenheight()
    x = max(8, min(px + (pw - ww) // 2, sw - ww - 8))
    y = max(8, min(py + (ph - wh) // 2, sh - wh - 8))
    win.geometry("+%d+%d" % (x, y))


class ToggleSwitch(tk.Canvas):
    """سوییچ کشویی مدرن با Canvas استاندارد Tk."""

    def __init__(self, parent, value=False, command=None, bg="#1b1f26",
                 on_color="#5fa8e8", off_color="#363d4a", knob_color="#eef1f6"):
        super().__init__(parent, width=34, height=18, bg=bg,
                         bd=0, highlightthickness=0, cursor="hand2")
        self._value = bool(value)
        self._command = command
        self._on_color = on_color
        self._off_color = off_color
        self._knob_color = knob_color
        self._draw()
        self.bind("<Button-1>", self._on_click)

    def get(self):
        return self._value

    def set(self, value):
        self._value = bool(value)
        self._draw()

    def _on_click(self, _event=None):
        self.toggle()

    def toggle(self):
        self._value = not self._value
        self._draw()
        if self._command:
            self._command(self._value)

    def _draw(self):
        self.delete("all")
        track = self._on_color if self._value else self._off_color
        self.create_line(9, 9, 25, 9, width=16, capstyle=tk.ROUND, fill=track)
        kx = 19 if self._value else 3
        self.create_oval(kx, 3, kx + 12, 15, fill=self._knob_color, outline="")


def _draw_icon(canvas, name, cx, cy, size, color, bg="#1d2128", angle=0.0):
    """رسم آیکون برداری سبک Lucide/FontAwesome روی Canvas بدون وابستگی خارجی."""
    r = size / 2.0
    lw = max(1.3, size * 0.115)

    if name in ("lock", "unlock"):
        bw, bh = r * 1.15, r * 0.9
        bx0, by0 = cx - bw / 2, cy - bh * 0.05
        canvas.create_rectangle(bx0, by0, bx0 + bw, by0 + bh,
                                outline=color, width=lw)
        sr = bw * 0.34
        if name == "lock":
            canvas.create_arc(cx - sr, by0 - sr * 1.75, cx + sr, by0 + sr * 0.25,
                              start=0, extent=180, style=tk.ARC, outline=color, width=lw)
            canvas.create_line(cx - sr, by0 - sr * 0.75, cx - sr, by0, fill=color, width=lw)
            canvas.create_line(cx + sr, by0 - sr * 0.75, cx + sr, by0, fill=color, width=lw)
        else:
            canvas.create_arc(cx - sr * 0.2, by0 - sr * 1.9, cx + sr * 1.8, by0 + sr * 0.1,
                              start=0, extent=180, style=tk.ARC, outline=color, width=lw)
            canvas.create_line(cx - sr * 0.2, by0 - sr * 0.9, cx - sr * 0.2, by0, fill=color, width=lw)
        canvas.create_oval(cx - 1.2, by0 + bh * 0.38 - 1.2,
                           cx + 1.2, by0 + bh * 0.38 + 1.2, fill=color, outline="")

    elif name == "sun":
        cr = r * 0.36
        canvas.create_oval(cx - cr, cy - cr, cx + cr, cy + cr, outline=color, width=lw)
        for i in range(8):
            a = math.radians(i * 45.0 + angle)
            x0, y0 = cx + math.cos(a) * (r * 0.60), cy + math.sin(a) * (r * 0.60)
            x1, y1 = cx + math.cos(a) * (r * 0.88), cy + math.sin(a) * (r * 0.88)
            canvas.create_line(x0, y0, x1, y1, fill=color, width=lw, capstyle=tk.ROUND)

    elif name == "moon":
        mr = r * 0.76
        canvas.create_oval(cx - mr, cy - mr, cx + mr, cy + mr, fill=color, outline="")
        canvas.create_oval(cx - mr * 0.38, cy - mr * 1.12,
                           cx + mr * 1.18, cy + mr * 0.44, fill=bg, outline="")

    elif name == "gear":
        ir = r * 0.28
        canvas.create_oval(cx - ir, cy - ir, cx + ir, cy + ir, outline=color, width=lw)
        orr = r * 0.54
        canvas.create_oval(cx - orr, cy - orr, cx + orr, cy + orr, outline=color, width=lw)
        for i in range(8):
            a = math.radians(i * 45.0 + angle)
            x0, y0 = cx + math.cos(a) * (r * 0.55), cy + math.sin(a) * (r * 0.55)
            x1, y1 = cx + math.cos(a) * (r * 0.86), cy + math.sin(a) * (r * 0.86)
            canvas.create_line(x0, y0, x1, y1, fill=color, width=lw * 1.25, capstyle=tk.ROUND)

    elif name == "close":
        d = r * 0.58
        canvas.create_line(cx - d, cy - d, cx + d, cy + d, fill=color, width=lw, capstyle=tk.ROUND)
        canvas.create_line(cx + d, cy - d, cx - d, cy + d, fill=color, width=lw, capstyle=tk.ROUND)

    elif name == "copy":
        s = r * 0.55
        canvas.create_rectangle(cx - s * 0.35, cy - s * 0.95, cx + s * 0.95, cy + s * 0.45,
                                outline=color, width=lw)
        canvas.create_rectangle(cx - s * 0.95, cy - s * 0.35, cx + s * 0.35, cy + s * 0.95,
                                fill=bg, outline=color, width=lw)

    elif name == "check":
        canvas.create_line(cx - r * 0.58, cy + r * 0.05,
                           cx - r * 0.14, cy + r * 0.52,
                           cx + r * 0.62, cy - r * 0.45,
                           fill=color, width=lw * 1.15, capstyle=tk.ROUND, joinstyle=tk.ROUND)

    elif name in ("eye", "eye_off"):
        w, h = r * 0.88, r * 0.55
        canvas.create_arc(cx - w, cy - h, cx + w, cy + h * 1.4,
                          start=20, extent=140, style=tk.ARC, outline=color, width=lw)
        canvas.create_arc(cx - w, cy - h * 1.4, cx + w, cy + h,
                          start=200, extent=140, style=tk.ARC, outline=color, width=lw)
        pr = r * 0.24
        canvas.create_oval(cx - pr, cy - pr, cx + pr, cy + pr, fill=color, outline="")
        if name == "eye_off":
            canvas.create_line(cx - r * 0.72, cy + r * 0.62, cx + r * 0.72, cy - r * 0.62,
                               fill=color, width=lw, capstyle=tk.ROUND)

    elif name == "key":
        kr = r * 0.34
        kx = cx - r * 0.38
        canvas.create_oval(kx - kr, cy - kr, kx + kr, cy + kr, outline=color, width=lw)
        canvas.create_line(kx + kr, cy, cx + r * 0.78, cy, fill=color, width=lw, capstyle=tk.ROUND)
        canvas.create_line(cx + r * 0.42, cy, cx + r * 0.42, cy + r * 0.36, fill=color, width=lw, capstyle=tk.ROUND)
        canvas.create_line(cx + r * 0.72, cy, cx + r * 0.72, cy + r * 0.28, fill=color, width=lw, capstyle=tk.ROUND)

    elif name == "clipboard":
        bw, bh = r * 1.1, r * 1.35
        canvas.create_rectangle(cx - bw / 2, cy - bh * 0.42, cx + bw / 2, cy + bh * 0.58,
                                outline=color, width=lw)
        canvas.create_rectangle(cx - bw * 0.26, cy - bh * 0.58, cx + bw * 0.26, cy - bh * 0.30,
                                fill=bg, outline=color, width=lw)

    elif name == "pause":
        h = r * 0.52
        canvas.create_line(cx - r * 0.26, cy - h, cx - r * 0.26, cy + h,
                           fill=color, width=lw * 1.2, capstyle=tk.ROUND)
        canvas.create_line(cx + r * 0.26, cy - h, cx + r * 0.26, cy + h,
                           fill=color, width=lw * 1.2, capstyle=tk.ROUND)

    elif name == "grip":
        for d in (3, 6, 9):
            canvas.create_line(cx + 6 - d, cy + 6, cx + 6, cy + 6 - d,
                               fill=color, width=1.2, capstyle=tk.ROUND)


LUCIDE_MAP = {
    "lock": "lock",
    "unlock": "lock-open",
    "sun": "sun",
    "moon": "moon",
    "gear": "settings",
    "close": "x",
    "copy": "copy",
    "check": "check",
    "eye": "eye",
    "eye_off": "eye-off",
    "key": "key-round",
    "clipboard": "clipboard",
    "pause": "pause",
    "spinner": "loader-circle",
    "plus": "plus",
    "trash": "trash-2",
    "refresh": "refresh-cw",
}


def _lucide_image(widget, name, size, color, angle=0.0):
    """ساخت و کش تصویر SVG واقعی از کتابخانه Lucide با tksvg."""
    if not HAS_LUCIDE or name not in LUCIDE_MAP:
        return None
    top = widget.winfo_toplevel()
    cache = getattr(top, "_lucide_cache", None)
    if cache is None:
        cache = {}
        top._lucide_cache = cache
    q_angle = (int(round(angle / 15.0)) * 15) % 360
    key = (name, size, color, q_angle)
    img = cache.get(key)
    if img is not None:
        return img
    el = deepcopy(lucide._load_icon(LUCIDE_MAP[name]))
    el.attrib["width"] = el.attrib["height"] = str(size)
    el.attrib["stroke"] = color
    el.attrib["stroke-width"] = "2"
    if q_angle:
        g = ET.Element("g", {"transform": "rotate(%d 12 12)" % q_angle})
        for ch in list(el):
            el.remove(ch)
            g.append(ch)
        el.append(g)
    svg = ET.tostring(el, encoding="unicode").replace(' xmlns="http://www.w3.org/2000/svg"', "", 1)
    img = tksvg.SvgImage(master=top, data=svg)
    cache[key] = img
    return img


class VectorButton(tk.Canvas):
    """دکمه‌ی آیکون برداری (Lucide SVG با fallback به Canvas) همراه با تم، هاور و انیمیشن."""

    def __init__(self, parent, icon, command, theme_fn,
                 bg_key="bar_bg", hover_bg_key="hover_bg",
                 fg_key="fg_muted", hover_fg_key="fg",
                 size=13, width=24, height=24, spin_on_hover=False, text=None):
        self._theme_fn = theme_fn
        self._bg_key = bg_key
        self._hover_bg_key = hover_bg_key
        self._fg_key = fg_key
        self._hover_fg_key = hover_fg_key
        self._icon = icon
        self._command = command
        self._size = size
        self._text = text
        self._spin_on_hover = spin_on_hover
        self._hovered = False
        self._angle = 0.0
        self._anim_id = None
        self._img = None
        t = theme_fn()
        super().__init__(parent, width=width, height=height, bg=self._col(t, bg_key),
                         bd=0, highlightthickness=0, cursor="hand2")
        self.bind("<Enter>", self._on_enter)
        self.bind("<Leave>", self._on_leave)
        self.bind("<Button-1>", lambda _e: self._command() if self._command else None)
        self.refresh()

    @staticmethod
    def _col(theme, key):
        return theme[key] if key in theme else key

    def set_icon(self, icon):
        self._icon = icon
        self.refresh()

    def set_fg_key(self, fg_key, hover_fg_key=None):
        self._fg_key = fg_key
        if hover_fg_key is not None:
            self._hover_fg_key = hover_fg_key
        self.refresh()

    def _on_enter(self, _e):
        self._hovered = True
        self.refresh()
        if self._spin_on_hover and self._anim_id is None:
            self._spin()

    def _on_leave(self, _e):
        self._hovered = False
        if self._anim_id is not None:
            self.after_cancel(self._anim_id)
            self._anim_id = None
        self._angle = 0.0
        self.refresh()

    def _spin(self):
        if not self._hovered:
            self._anim_id = None
            return
        self._angle = (self._angle + 15.0) % 360.0
        self.refresh()
        self._anim_id = self.after(40, self._spin)

    def refresh(self):
        t = self._theme_fn()
        bg = self._col(t, self._hover_bg_key if self._hovered else self._bg_key)
        fg = self._col(t, self._hover_fg_key if self._hovered else self._fg_key)
        self.configure(bg=bg)
        self.delete("all")
        w = int(self["width"])
        h = int(self["height"])
        cx = (w - 12) if self._text else (w / 2.0)
        cy = h / 2.0
        img = _lucide_image(self, self._icon, self._size + 1, fg, angle=self._angle)
        if img is not None:
            self._img = img
            self.create_image(cx, cy, image=img)
        else:
            _draw_icon(self, self._icon, cx, cy, self._size, fg, bg=bg, angle=self._angle)
        if self._text:
            self.create_text(cx - 12, cy, text=self._text, fill=fg,
                             font=("Segoe UI", 8), anchor="e")


class StatusSpinner(tk.Canvas):
    """نشانگر انیمیشن‌دار وضعیت ترجمه در نوار پایین."""

    def __init__(self, parent, theme_fn):
        self._theme_fn = theme_fn
        self._state = "idle"
        self._angle = 0
        self._job = None
        self._img = None
        t = theme_fn()
        super().__init__(parent, width=14, height=14, bg=t["bg"],
                         bd=0, highlightthickness=0)

    def set_state(self, state):
        self._state = state
        if self._job is not None:
            self.after_cancel(self._job)
            self._job = None
        if state == "loading":
            self._tick()
        else:
            self.refresh()

    def _tick(self):
        if self._state != "loading":
            self._job = None
            return
        self._angle = (self._angle + 30) % 360
        self.refresh()
        self._job = self.after(40, self._tick)

    def refresh(self):
        t = self._theme_fn()
        self.configure(bg=t["bg"])
        self.delete("all")
        if self._state == "loading":
            img = _lucide_image(self, "spinner", 13, t["accent"], angle=self._angle)
            if img is not None:
                self._img = img
                self.create_image(7, 7, image=img)
            else:
                self.create_arc(2, 2, 12, 12, start=-self._angle, extent=250,
                                style=tk.ARC, outline=t["accent"], width=1.8)
        elif self._state == "ok":
            img = _lucide_image(self, "check", 13, t["accent_green"])
            if img is not None:
                self._img = img
                self.create_image(7, 7, image=img)
            else:
                _draw_icon(self, "check", 7, 7, 10, t["accent_green"], bg=t["bg"])
        elif self._state == "err":
            img = _lucide_image(self, "close", 12, t["err_fg"])
            if img is not None:
                self._img = img
                self.create_image(7, 7, image=img)
            else:
                _draw_icon(self, "close", 7, 7, 9, t["err_fg"], bg=t["bg"])


class Overlay:
    def __init__(self, root, cfg):
        self.root, self.cfg = root, cfg
        self.events = queue.Queue()
        self.last_text = None
        self.last_ok = False
        self.last_out = None
        self.last_tag = None
        self.seq = 0
        self.clip_seq = 0
        self.dragging = False
        self.resizing = False
        self._ctx_menu = None
        self._build()
        self.watcher = MousePoller(self._grab_selection)
        self.watcher.enabled = bool(cfg.get("watch", True))
        self.root.after(MOUSE_POLL_MS, self._poll_mouse)
        self.root.after(POLL_MS, self._pump)
        self.root.after(4000, self._keep_topmost)
        if not cfg["api_key"].strip():
            self.root.after(300, self.open_settings)

    def _poll_mouse(self):
        try:
            self.watcher.poll()
        except Exception as exc:                      # noqa: BLE001 - نباید حلقه بمیرد
            log("poll error: %r" % (exc,))
        self.root.after(MOUSE_POLL_MS, self._poll_mouse)

    def _theme(self):
        return THEMES.get(self.cfg.get("theme", "dark"), THEMES["dark"])

    # ---- layout
    def _build(self):
        r, cfg = self.root, self.cfg
        t = self._theme()
        r.overrideredirect(True)
        r.attributes("-topmost", True)
        r.attributes("-alpha", float(cfg.get("opacity", 1.0)))
        w = _int(cfg.get("width"), WIDTH, 340, 800)
        h = _int(cfg.get("height"), HEIGHT, 170, 600)
        sw, sh = r.winfo_screenwidth(), r.winfo_screenheight()
        x = min(max(int(cfg["x"]), 0), max(sw - w, 0))
        y = min(max(int(cfg["y"]), 0), max(sh - h, 0))
        r.geometry("%dx%d+%d+%d" % (w, h, x, y))
        r.configure(bg=t["border"])

        self.shell = tk.Frame(r, bg=t["bg"])
        self.shell.pack(fill="both", expand=True, padx=1, pady=1)

        self.bar = tk.Frame(self.shell, bg=t["bar_bg"], height=30)
        self.bar.pack(fill="x")
        self.bar.pack_propagate(False)
        self.bar_div = tk.Frame(self.shell, bg=t["divider"], height=1)
        self.bar_div.pack(fill="x")

        self.bar.bind("<Button-1>", self._drag_start)
        self.bar.bind("<B1-Motion>", self._drag_move)
        self.bar.bind("<ButtonRelease-1>", self._drag_end)
        self.title_lbl = tk.Label(self.bar, text=" مترجم", bg=t["bar_bg"], fg=t["fg_muted"],
                                  font=("Segoe UI", 9, "bold"))
        self.title_lbl.pack(side="left", padx=(4, 0))
        self.title_lbl.bind("<Button-1>", self._drag_start)
        self.title_lbl.bind("<B1-Motion>", self._drag_move)
        self.title_lbl.bind("<ButtonRelease-1>", self._drag_end)

        self.lock_btn = VectorButton(
            self.bar, "lock" if cfg["locked"] else "unlock", self.toggle_lock,
            self._theme, fg_key="accent_amber" if cfg["locked"] else "fg_muted",
            hover_fg_key="fg", size=12, width=22, height=24)
        self.lock_btn.pack(side="left", padx=(2, 0))

        self.model_var = tk.StringVar(value=MODELS[cfg["model"]][1] + " ▾")
        self.model_pick = tk.StringVar(value=cfg["model"])
        self.model_btn = tk.Menubutton(self.bar, textvariable=self.model_var, bd=0,
                                       bg=t["bar_bg"], fg=t["accent"],
                                       activebackground=t["hover_bg"], activeforeground=t["fg"],
                                       font=("Segoe UI", 8), indicatoron=False,
                                       padx=5, cursor="hand2")
        self.model_menu = tk.Menu(self.model_btn, tearoff=0)
        _style_menu(self.model_menu, t)
        for mid, (_engine, _short, label) in MODELS.items():
            self.model_menu.add_radiobutton(label=label, value=mid, variable=self.model_pick,
                                            command=lambda m=mid: self.set_model(m))
        self.model_btn.configure(menu=self.model_menu)
        self.model_btn.pack(side="left", padx=(2, 0))
        _bind_hover(self.model_btn, self._theme, "bar_bg", "hover_bg")

        self.key_var = tk.StringVar()
        self.key_pick = tk.StringVar(value=cfg["api_key"])
        self.key_btn = tk.Menubutton(self.bar, textvariable=self.key_var, bd=0,
                                     bg=t["bar_bg"], fg=t["key_fg"],
                                     activebackground=t["hover_bg"], activeforeground=t["fg"],
                                     font=("Segoe UI", 8), indicatoron=False,
                                     padx=4, cursor="hand2")
        self.key_menu = tk.Menu(self.key_btn, tearoff=0)
        _style_menu(self.key_menu, t)
        self.key_btn.configure(menu=self.key_menu)
        self.key_btn.pack(side="left", padx=(2, 0))
        _bind_hover(self.key_btn, self._theme, "bar_bg", "hover_bg")
        self._refresh_keys()

        self.rtl_var = tk.StringVar(value="RTL")
        self.rtl_btn = tk.Button(self.bar, textvariable=self.rtl_var, bd=0, bg=t["bar_bg"],
                                 fg=t["fg_muted"], activebackground=t["hover_bg"],
                                 font=("Segoe UI", 8), padx=4, cursor="hand2",
                                 command=self.toggle_rtl)
        self.rtl_btn.pack(side="left", padx=(2, 0))
        _bind_hover(self.rtl_btn, self._theme, "bar_bg", "hover_bg")

        self.close_btn = VectorButton(
            self.bar, "close", r.destroy, self._theme,
            hover_bg_key="close_hover", hover_fg_key="#ffffff",
            size=11, width=24, height=24)
        self.close_btn.pack(side="right", padx=(0, 2))

        self.cfg_btn = VectorButton(
            self.bar, "gear", self.open_settings, self._theme,
            size=13, width=24, height=24, spin_on_hover=True)
        self.cfg_btn.pack(side="right")

        self.theme_btn = VectorButton(
            self.bar, "sun" if cfg.get("theme", "dark") == "dark" else "moon",
            self.toggle_theme, self._theme,
            size=13, width=24, height=24, spin_on_hover=True)
        self.theme_btn.pack(side="right")

        self.copy_btn = VectorButton(
            self.bar, "copy", self.copy_translation, self._theme,
            size=12, width=48, height=24, text="کپی")
        self.copy_btn.pack(side="right", padx=(0, 2))

        self.src_text = tk.Text(self.shell, bd=0, bg=t["bg"], fg=t["fg_muted"],
                                font=("Segoe UI", 9), wrap="word", padx=2, pady=2,
                                height=1, insertwidth=0, highlightthickness=0,
                                selectbackground=t["select_bg"], cursor="arrow")
        self.src_text.pack(fill="x", padx=8, pady=(6, 2))
        self.src_text.tag_configure("src_rtl", justify="right")
        self.src_text.tag_configure("src_ltr", justify="left")
        self._set_source("متنی را انتخاب کنید…")

        # نوار پایین قبل از کارت خروجی pack می‌شود تا هیچ‌وقت بریده نشود
        self.foot = tk.Frame(self.shell, bg=t["bg"], height=22)
        self.foot.pack(side="bottom", fill="x", padx=8, pady=(0, 4))
        self.foot.pack_propagate(False)

        self.grip = VectorButton(
            self.foot, "grip", None, self._theme,
            bg_key="bg", hover_bg_key="bg", fg_key="fg_dim", hover_fg_key="fg_muted",
            size=12, width=16, height=16)
        self.grip.configure(cursor="size_nw_se")
        self.grip.pack(side="right", padx=(4, 0))
        self.grip.bind("<Button-1>", self._resize_start)
        self.grip.bind("<B1-Motion>", self._resize_move)
        self.grip.bind("<ButtonRelease-1>", self._resize_end)

        self.spinner = StatusSpinner(self.foot, self._theme)
        self.spinner.pack(side="right", padx=(4, 2))

        self.status_var = tk.StringVar(value="آماده")
        self.status_lbl = tk.Label(self.foot, textvariable=self.status_var, bg=t["bg"], fg=t["fg_dim"],
                                   font=("Segoe UI", 8), anchor="e")
        self.status_lbl.pack(side="right")

        self.watch_badge_var = tk.StringVar(
            value="" if cfg.get("watch", True) else "ترجمه خودکار خاموش")
        self.watch_badge = tk.Label(self.foot, textvariable=self.watch_badge_var, bg=t["bg"],
                                    fg=t["accent_amber"], font=("Segoe UI", 8),
                                    cursor="hand2")
        self.watch_badge.pack(side="left")
        self.watch_badge.bind("<Button-1>", lambda _e: self._set_watch(not self.watch_var.get()))

        self.out_card = tk.Frame(self.shell, bg=t["card_bg"], highlightthickness=1,
                                 highlightbackground=t["card_border"])
        self.out_card.pack(fill="both", expand=True, padx=8, pady=(2, 4))
        self.out = tk.Text(self.out_card, bd=0, bg=t["card_bg"], fg=t["fg"], font=("Segoe UI", 12),
                           wrap="none", padx=8, pady=6, height=4, insertwidth=0,
                           highlightthickness=0, selectbackground=t["select_bg"])
        self.out.pack(fill="both", expand=True)
        self.out.configure(state="disabled")
        self.out_font = tkfont.Font(root=r, font=self.out["font"])
        self._layout_source = []
        self.out.tag_configure("rtl", justify="right")
        self.out.tag_configure("err", foreground=t["err_fg"])

        self.autocopy_var = tk.BooleanVar(value=bool(cfg.get("autocopy")))
        self.watch_var = tk.BooleanVar(value=bool(cfg.get("watch", True)))

        for wgt in (r, self.shell, self.bar, self.title_lbl, self.src_text,
                    self.out_card, self.out, self.foot, self.status_lbl):
            wgt.bind("<Button-3>", self._popup)
        r.bind("<Control-Shift-C>", self.copy_translation)
        r.bind("<Control-Shift-c>", self.copy_translation)
        self.out.bind("<Control-c>", self._copy_out)

    def _set_source(self, text):
        tag = "src_rtl" if _is_rtl_text(text) else "src_ltr"
        self.src_text.configure(state="normal")
        self.src_text.delete("1.0", "end")
        self.src_text.insert("1.0", text, (tag,))
        self.src_text.configure(state="disabled")
        self._update_src_height()

    def _update_src_height(self):
        try:
            self.src_text.update_idletasks()
            res = self.src_text.count("1.0", "end", "displaylines")
            lines = int(res[0]) if res else 1
        except (tk.TclError, TypeError, ValueError):
            lines = 1
        win_h = self.root.winfo_height()
        if win_h <= 1:
            win_h = _int(self.cfg.get("height"), HEIGHT, 170, 600)
        max_lines = max(2, min(5, (win_h - 125) // 18))
        self.src_text.configure(height=max(1, min(lines, max_lines)))

    def apply_theme(self):
        t = self._theme()
        is_dark = self.cfg.get("theme", "dark") == "dark"
        self.root.configure(bg=t["border"])
        self.shell.configure(bg=t["bg"])
        self.bar.configure(bg=t["bar_bg"])
        self.bar_div.configure(bg=t["divider"])
        self.title_lbl.configure(bg=t["bar_bg"], fg=t["fg_muted"])
        self.lock_btn.set_fg_key("accent_amber" if self.cfg["locked"] else "fg_muted", "fg")
        self.model_btn.configure(bg=t["bar_bg"], fg=t["accent"],
                                 activebackground=t["hover_bg"], activeforeground=t["fg"])
        _style_menu(self.model_menu, t)
        self.key_btn.configure(bg=t["bar_bg"], fg=t["key_fg"],
                               activebackground=t["hover_bg"], activeforeground=t["fg"])
        _style_menu(self.key_menu, t)
        self.rtl_btn.configure(bg=t["bar_bg"],
                               fg=t["accent"] if self._rtl() else t["fg_muted"],
                               activebackground=t["hover_bg"])
        self.theme_btn.set_icon("sun" if is_dark else "moon")
        self.copy_btn.set_fg_key("accent" if self.last_ok else "fg_muted", "fg")
        self.cfg_btn.refresh()
        self.close_btn.refresh()
        self.src_text.configure(bg=t["bg"], fg=t["fg_muted"], selectbackground=t["select_bg"])
        self.out_card.configure(bg=t["card_bg"], highlightbackground=t["card_border"])
        self.out.configure(bg=t["card_bg"], fg=t["fg"], selectbackground=t["select_bg"])
        self.out.tag_configure("err", foreground=t["err_fg"])
        self.foot.configure(bg=t["bg"])
        self.grip.refresh()
        self.spinner.refresh()
        self.status_lbl.configure(bg=t["bg"], fg=t["fg_dim"])
        self.watch_badge.configure(bg=t["bg"], fg=t["accent_amber"])

    def toggle_theme(self):
        self.cfg["theme"] = "light" if self.cfg.get("theme", "dark") == "dark" else "dark"
        save_config(self.cfg)
        self.apply_theme()
        self.status_var.set("تم: " + ("تاریک" if self.cfg["theme"] == "dark" else "روشن"))

    def _close_popup(self):
        if self._ctx_menu is not None:
            try:
                self._ctx_menu.destroy()
            except tk.TclError:
                pass
            self._ctx_menu = None

    def _popup(self, event):
        self._close_popup()
        t = self._theme()
        pop = tk.Toplevel(self.root)
        pop.overrideredirect(True)
        pop.attributes("-topmost", True)
        pop.configure(bg=t["border"])
        self._ctx_menu = pop

        card = tk.Frame(pop, bg=t["card_bg"], padx=8, pady=8)
        card.pack(fill="both", expand=True, padx=1, pady=1)

        # ردیف دکمه‌های سریع
        actions = tk.Frame(card, bg=t["card_bg"])
        actions.pack(fill="x", pady=(0, 6))

        def _act(fn):
            self._close_popup()
            fn()

        for icon, text, fn in (("gear", "تنظیمات", self.open_settings),
                               ("clipboard", "کلیپ‌بورد", self._from_clipboard),
                               ("copy", "کپی", self.copy_translation)):
            b = VectorButton(actions, icon, lambda f=fn: _act(f), self._theme,
                             bg_key="btn_bg", hover_bg_key="hover_bg",
                             fg_key="fg", hover_fg_key="fg",
                             size=11, width=78, height=26, text=text)
            b.pack(side="left", expand=True, fill="x", padx=2)

        tk.Frame(card, bg=t["divider"], height=1).pack(fill="x", pady=(2, 6))

        # ردیف سوییچ‌های کشویی (بدون متن اضافی)
        def add_toggle(label, initial, on_toggle):
            row = tk.Frame(card, bg=t["card_bg"], cursor="hand2", padx=4, pady=3)
            row.pack(fill="x", pady=1)
            sw = ToggleSwitch(row, value=initial, bg=t["card_bg"],
                              on_color=t["accent"], off_color=t["switch_off"],
                              knob_color=t["knob"])
            sw.pack(side="left", padx=(0, 8))
            lbl = tk.Label(row, text=label, bg=t["card_bg"], fg=t["fg"],
                           font=("Segoe UI", 9), anchor="e")
            lbl.pack(side="right", fill="x", expand=True)

            def _flip(_e=None):
                val = not sw.get()
                sw.set(val)
                on_toggle(val)

            sw._command = on_toggle
            row.bind("<Button-1>", _flip)
            lbl.bind("<Button-1>", _flip)

            def _enter(_e):
                cur = self._theme()
                row.configure(bg=cur["hover_bg"])
                lbl.configure(bg=cur["hover_bg"])
                sw.configure(bg=cur["hover_bg"])

            def _leave(_e):
                cur = self._theme()
                row.configure(bg=cur["card_bg"])
                lbl.configure(bg=cur["card_bg"])
                sw.configure(bg=cur["card_bg"])

            for wgt in (row, lbl, sw):
                wgt.bind("<Enter>", _enter, add="+")
                wgt.bind("<Leave>", _leave, add="+")

        add_toggle("ترجمه خودکار", bool(self.cfg.get("watch", True)), self._set_watch)
        add_toggle("کپی خودکار", bool(self.cfg.get("autocopy")), self._set_autocopy)
        add_toggle("قفل پنجره", bool(self.cfg.get("locked")), lambda _v: self.toggle_lock())
        add_toggle("راست‌به‌چپ", self._rtl(), lambda _v: self.toggle_rtl())
        add_toggle("حالت روشن", self.cfg.get("theme", "dark") == "light",
                   lambda _v: _act(self.toggle_theme))

        tk.Frame(card, bg=t["divider"], height=1).pack(fill="x", pady=(6, 6))

        # شفافیت سریع
        op_row = tk.Frame(card, bg=t["card_bg"], padx=4)
        op_row.pack(fill="x", pady=(0, 6))
        tk.Label(op_row, text="شفافیت", bg=t["card_bg"], fg=t["fg_muted"],
                 font=("Segoe UI", 8)).pack(side="right")
        cur_op = round(float(self.cfg.get("opacity", 1.0)), 2)
        op_btns = []

        def set_opacity(val):
            self.cfg["opacity"] = val
            self.root.attributes("-alpha", val)
            save_config(self.cfg)
            cur = self._theme()
            for v, btn in op_btns:
                active = abs(v - val) < 0.03
                btn.configure(bg=cur["accent"] if active else cur["btn_bg"],
                              fg=cur["accent_fg"] if active else cur["fg_muted"])

        for val, txt in ((0.7, "70%"), (0.85, "85%"), (1.0, "100%")):
            active = abs(val - cur_op) < 0.03
            b = tk.Button(op_row, text=txt, bd=0,
                          bg=t["accent"] if active else t["btn_bg"],
                          fg=t["accent_fg"] if active else t["fg_muted"],
                          font=("Segoe UI", 8), padx=6, pady=2, cursor="hand2",
                          command=lambda v=val: set_opacity(v))
            b.pack(side="left", padx=2)
            op_btns.append((val, b))

        tk.Frame(card, bg=t["divider"], height=1).pack(fill="x", pady=(0, 6))

        # ردیف پایین: خروج + کلیدها
        foot = tk.Frame(card, bg=t["card_bg"])
        foot.pack(fill="x")
        exit_btn = tk.Button(foot, text="خروج", bd=0, bg=t["card_bg"], fg=t["accent_red"],
                             activebackground=t["close_hover"], activeforeground="#ffffff",
                             font=("Segoe UI", 8), padx=8, pady=3, cursor="hand2",
                             command=lambda: _act(self.root.destroy))
        exit_btn.pack(side="left")
        _bind_hover(exit_btn, self._theme, "card_bg", "close_hover", "accent_red", "#ffffff")

        keys_btn = VectorButton(foot, "key", lambda: _act(self.open_keys), self._theme,
                                bg_key="card_bg", hover_bg_key="hover_bg",
                                fg_key="fg_muted", hover_fg_key="fg",
                                size=11, width=106, height=24, text="کلیدها و مصرف")
        keys_btn.pack(side="right")

        pop.update_idletasks()
        pw, ph = pop.winfo_reqwidth(), pop.winfo_reqheight()
        sw, sh = self.root.winfo_screenwidth(), self.root.winfo_screenheight()
        x = min(max(event.x_root, 4), max(sw - pw - 4, 4))
        y = min(max(event.y_root, 4), max(sh - ph - 4, 4))
        pop.geometry("+%d+%d" % (x, y))
        pop.focus_force()
        pop.bind("<Escape>", lambda _e: self._close_popup())

        def _on_focus_out(_e):
            def _check():
                if self._ctx_menu is not pop:
                    return
                try:
                    focused = pop.focus_get()
                    if focused is None or focused.winfo_toplevel() is not pop:
                        self._close_popup()
                except (tk.TclError, KeyError):
                    self._close_popup()
            pop.after(20, _check)

        pop.bind("<FocusOut>", _on_focus_out)

    # ---- resizing the window
    def _resize_start(self, event):
        self.resizing = True
        self._rx, self._ry = event.x_root, event.y_root
        self._rw, self._rh = self.root.winfo_width(), self.root.winfo_height()

    def _resize_move(self, event):
        if not self.resizing:
            return
        nw = max(340, min(800, self._rw + (event.x_root - self._rx)))
        nh = max(170, min(600, self._rh + (event.y_root - self._ry)))
        self.root.geometry("%dx%d" % (nw, nh))
        self._update_src_height()

    def _resize_end(self, _event):
        if not self.resizing:
            return
        self.resizing = False
        self.cfg["width"] = self.root.winfo_width()
        self.cfg["height"] = self.root.winfo_height()
        save_config(self.cfg)
        self._update_src_height()
        if self.last_out is not None:
            self._set_out(self.last_out, self.last_tag)

    # ---- moving the window (only by hand)
    def _drag_start(self, event):
        if self.cfg["locked"]:
            return
        self.dragging = True
        self._gx, self._gy = event.x_root - self.root.winfo_x(), event.y_root - self.root.winfo_y()

    def _drag_move(self, event):
        if self.dragging and not self.cfg["locked"]:
            self.root.geometry("+%d+%d" % (event.x_root - self._gx, event.y_root - self._gy))

    def _drag_end(self, _event):
        if not self.dragging:
            return
        self.dragging = False
        self.cfg["x"], self.cfg["y"] = self.root.winfo_x(), self.root.winfo_y()
        save_config(self.cfg)
        self.status_var.set("موقعیت ذخیره شد")

    def toggle_lock(self):
        self.cfg["locked"] = not self.cfg["locked"]
        self.lock_btn.set_icon("lock" if self.cfg["locked"] else "unlock")
        self.lock_btn.set_fg_key("accent_amber" if self.cfg["locked"] else "fg_muted", "fg")
        save_config(self.cfg)
        self.status_var.set("جابه‌جایی قفل است" if self.cfg["locked"] else "جابه‌جایی آزاد است")

    def _refresh_keys(self):
        keys = self.cfg.get("api_keys", [])
        self.key_pick.set(self.cfg.get("api_key", ""))
        self.key_var.set(key_label(self.cfg.get("api_key", "")) + " ▾")
        self.key_menu.delete(0, "end")
        for key in keys:
            self.key_menu.add_radiobutton(label=key_label(key), value=key,
                                          variable=self.key_pick,
                                          command=lambda k=key: self.set_key(k))
        if keys:
            self.key_menu.add_separator()
        self.key_menu.add_command(label="کلیدها و مصرف…", command=self.open_keys)

    def set_key(self, key):
        if key not in self.cfg.get("api_keys", []):
            return
        self.cfg["api_key"] = key
        save_config(self.cfg)
        self._refresh_keys()
        if self.last_text and not self.last_ok:
            self._translate_text(self.last_text)
        else:
            self.last_text = None
            self.status_var.set("کلید فعال: " + key_label(key))

    def open_keys(self):
        t = self._theme()
        win = tk.Toplevel(self.root)
        win.title("کلیدها و مصرف")
        win.overrideredirect(True)
        win.configure(bg=t["border"])
        win.attributes("-topmost", True)

        shell = tk.Frame(win, bg=t["bg"])
        shell.pack(fill="both", expand=True, padx=1, pady=1)

        # نوار عنوان هم‌سبک با پنجره اصلی
        bar = tk.Frame(shell, bg=t["bar_bg"], height=32)
        bar.pack(fill="x")
        bar.pack_propagate(False)
        tk.Frame(shell, bg=t["divider"], height=1).pack(fill="x")

        drag = {"x": 0, "y": 0, "on": False}

        def _d_start(e):
            drag["on"] = True
            drag["x"], drag["y"] = e.x_root - win.winfo_x(), e.y_root - win.winfo_y()

        def _d_move(e):
            if drag["on"]:
                win.geometry("+%d+%d" % (e.x_root - drag["x"], e.y_root - drag["y"]))

        def _d_end(_e):
            drag["on"] = False

        hdr_icon = VectorButton(bar, "key", None, self._theme, bg_key="bar_bg",
                                hover_bg_key="bar_bg", fg_key="accent",
                                hover_fg_key="accent", size=13, width=24, height=24)
        hdr_icon.configure(cursor="arrow")
        hdr_icon.pack(side="left", padx=(6, 0))
        hdr_lbl = tk.Label(bar, text="کلیدها و مصرف", bg=t["bar_bg"], fg=t["fg"],
                           font=("Segoe UI", 9, "bold"))
        hdr_lbl.pack(side="left", padx=(2, 0))
        for wgt in (bar, hdr_lbl):
            wgt.bind("<Button-1>", _d_start)
            wgt.bind("<B1-Motion>", _d_move)
            wgt.bind("<ButtonRelease-1>", _d_end)

        close_btn = VectorButton(bar, "close", win.destroy, self._theme,
                                 hover_bg_key="close_hover", hover_fg_key="#ffffff",
                                 size=11, width=26, height=26)
        close_btn.pack(side="right", padx=(0, 4))

        body = tk.Frame(shell, bg=t["bg"], padx=12, pady=10)
        body.pack(fill="both", expand=True)

        # ---- کارت ۱: مدیریت کلیدها
        card1 = tk.Frame(body, bg=t["card_bg"], highlightthickness=1,
                         highlightbackground=t["card_border"], padx=10, pady=8)
        card1.pack(fill="x", pady=(0, 8))

        status = tk.StringVar(value="\u200fکلیدها فقط در config.json همین کامپیوتر ذخیره می‌شوند.")
        c1_hdr = tk.Frame(card1, bg=t["card_bg"])
        c1_hdr.pack(fill="x", pady=(0, 6))
        tk.Label(c1_hdr, text="مدیریت کلیدهای API", bg=t["card_bg"], fg=t["fg"],
                 font=("Segoe UI", 9, "bold")).pack(side="right")
        tk.Label(c1_hdr, textvariable=status, bg=t["card_bg"], fg=t["accent"],
                 font=("Segoe UI", 8)).pack(side="left")

        box = tk.Listbox(card1, height=4, bg=t["bg"], fg=t["fg"],
                         selectbackground=t["select_bg"], selectforeground=t["fg"],
                         bd=0, exportselection=False, highlightthickness=1,
                         highlightbackground=t["border"], font=("Segoe UI", 9))
        box.pack(fill="x", pady=(0, 8))

        ctrl_row = tk.Frame(card1, bg=t["card_bg"])
        ctrl_row.pack(fill="x")

        new_var = tk.StringVar()
        entry = _themed_entry(ctrl_row, t, new_var, width=36, show="•")
        entry.pack(side="left", fill="x", expand=True)

        shown_new = [False]

        def toggle_new_vis():
            shown_new[0] = not shown_new[0]
            entry.configure(show="" if shown_new[0] else "•")
            eye_btn.set_icon("eye_off" if shown_new[0] else "eye")
            eye_btn.set_fg_key("accent" if shown_new[0] else "fg_muted", "fg")

        eye_btn = VectorButton(ctrl_row, "eye", toggle_new_vis, self._theme,
                               bg_key="card_bg", hover_bg_key="hover_bg",
                               size=13, width=26, height=24)
        eye_btn.pack(side="left", padx=(4, 8))

        def selected():
            pick = box.curselection()
            return self.cfg["api_keys"][pick[0]] if pick else None

        only_active = [True]

        def refresh():
            box.delete(0, "end")
            for key in self.cfg["api_keys"]:
                mark = "   ✓ فعال" if key == self.cfg["api_key"] else ""
                box.insert("end", "  " + key_label(key) + mark)
            for iid in tree.get_children():
                tree.delete(iid)
            keys = [self.cfg["api_key"]] if (only_active[0] and self.cfg["api_key"]) else self.cfg["api_keys"]
            for key, model, rpm, tpm, rpd, limits in usage_rows(keys):
                lr, lt, ld = limits
                s_rpm = f"{rpm} / {'—' if lr is None else lr}"
                s_tpm = f"{tpm:,} / {'—' if lt is None else f'{lt:,}'}"
                s_rpd = f"{rpd} / {'—' if ld is None else ld}"
                tag = "idle"
                if (ld and rpd >= ld) or (lr and rpm >= lr) or (lt and tpm >= lt):
                    tag = "full"
                elif (ld and rpd >= ld * 0.8) or (lr and rpm >= lr * 0.8):
                    tag = "warn"
                elif rpd > 0 or rpm > 0 or tpm > 0:
                    tag = "used"
                tree.insert("", "end", values=(key_label(key), MODELS[model][2], s_rpm, s_tpm, s_rpd), tags=(tag,))

        def add():
            key = new_var.get().strip()
            if not key:
                status.set("کلید جدید را وارد کنید.")
                return
            if key not in self.cfg["api_keys"]:
                self.cfg["api_keys"].append(key)
            self.cfg["api_key"] = key
            new_var.set("")
            save_config(self.cfg)
            self._refresh_keys()
            refresh()
            status.set("کلید افزوده و فعال شد ✓")

        def remove():
            key = selected()
            if not key:
                status.set("ابتدا یک کلید را از فهرست انتخاب کنید.")
                return
            self.cfg["api_keys"].remove(key)
            if self.cfg["api_key"] == key:
                self.cfg["api_key"] = self.cfg["api_keys"][0] if self.cfg["api_keys"] else ""
            save_config(self.cfg)
            self._refresh_keys()
            refresh()
            status.set("کلید حذف شد.")

        def activate():
            key = selected()
            if key:
                self.set_key(key)
                refresh()
                status.set("کلید فعال تغییر کرد ✓")

        add_btn = VectorButton(ctrl_row, "plus", add, self._theme,
                               bg_key="accent", hover_bg_key="accent_hover",
                               fg_key="accent_fg", hover_fg_key="accent_fg",
                               size=11, width=74, height=26, text="افزودن")
        add_btn.pack(side="left", padx=(0, 4))
        act_btn = VectorButton(ctrl_row, "check", activate, self._theme,
                               bg_key="btn_bg", hover_bg_key="hover_bg",
                               fg_key="fg", hover_fg_key="fg",
                               size=11, width=82, height=26, text="فعال‌سازی")
        act_btn.pack(side="left", padx=(0, 4))
        del_btn = VectorButton(ctrl_row, "trash", remove, self._theme,
                               bg_key="btn_bg", hover_bg_key="close_hover",
                               fg_key="accent_red", hover_fg_key="#ffffff",
                               size=11, width=64, height=26, text="حذف")
        del_btn.pack(side="left")

        # ---- کارت ۲: جدول آمار مصرف
        card2 = tk.Frame(body, bg=t["card_bg"], highlightthickness=1,
                         highlightbackground=t["card_border"], padx=10, pady=8)
        card2.pack(fill="both", expand=True, pady=(0, 8))

        c2_hdr = tk.Frame(card2, bg=t["card_bg"])
        c2_hdr.pack(fill="x", pady=(0, 6))
        tk.Label(c2_hdr, text="آمار مصرف (۲۴ ساعت گذشته)", bg=t["card_bg"], fg=t["fg"],
                 font=("Segoe UI", 9, "bold")).pack(side="right")

        ref_btn = VectorButton(c2_hdr, "refresh", refresh, self._theme,
                               bg_key="btn_bg", hover_bg_key="hover_bg",
                               fg_key="fg", hover_fg_key="fg",
                               size=11, width=92, height=24, spin_on_hover=True, text="به‌روزرسانی")
        ref_btn.pack(side="left")

        def _toggle_scope(val):
            only_active[0] = bool(val)
            refresh()

        scope_sw = ToggleSwitch(c2_hdr, value=True, command=_toggle_scope, bg=t["card_bg"],
                                on_color=t["accent"], off_color=t["switch_off"],
                                knob_color=t["knob"])
        scope_sw.pack(side="left", padx=(12, 4))
        tk.Label(c2_hdr, text="فقط کلید فعال", bg=t["card_bg"], fg=t["fg_muted"],
                 font=("Segoe UI", 8)).pack(side="left")

        style = ttk.Style(win)
        try:
            style.theme_use("clam")
        except tk.TclError:
            pass
        style.configure("Usage.Treeview",
                        background=t["bg"], fieldbackground=t["bg"],
                        foreground=t["fg"], bordercolor=t["border"],
                        rowheight=24, font=("Segoe UI", 9))
        style.configure("Usage.Treeview.Heading",
                        background=t["bar_bg"], foreground=t["fg_muted"],
                        bordercolor=t["divider"], relief="flat",
                        font=("Segoe UI", 8, "bold"))
        style.map("Usage.Treeview",
                  background=[("selected", t["select_bg"])],
                  foreground=[("selected", t["fg"])])
        style.map("Usage.Treeview.Heading",
                  background=[("active", t["hover_bg"])])

        tree_wrap = tk.Frame(card2, bg=t["border"], padx=1, pady=1)
        tree_wrap.pack(fill="both", expand=True)

        cols = ("key", "model", "rpm", "tpm", "rpd")
        tree = ttk.Treeview(tree_wrap, columns=cols, show="headings",
                            height=11, style="Usage.Treeview")
        for col, title_txt, col_w, anc in (
            ("key", "کلید", 90, "center"),
            ("model", "مدل", 185, "w"),
            ("rpm", "دقیقه (RPM)", 105, "center"),
            ("tpm", "توکن/دقیقه (TPM)", 145, "center"),
            ("rpd", "۲۴ ساعت (RPD)", 115, "center"),
        ):
            tree.heading(col, text=title_txt, anchor=anc)
            tree.column(col, width=col_w, anchor=anc, stretch=(col == "model"))

        tree.tag_configure("idle", foreground=t["fg_muted"])
        tree.tag_configure("used", foreground=t["accent"])
        tree.tag_configure("warn", foreground=t["accent_amber"])
        tree.tag_configure("full", foreground=t["err_fg"])

        sb = tk.Scrollbar(tree_wrap, orient="vertical", command=tree.yview,
                          bg=t["bar_bg"], troughcolor=t["bg"], bd=0, highlightthickness=0)
        tree.configure(yscrollcommand=sb.set)
        tree.pack(side="left", fill="both", expand=True)
        sb.pack(side="right", fill="y")

        # نکته‌ی پایین با جهت راست‌به‌چپ صحیح
        note = ("\u200fمصرف فقط مربوط به همین برنامه و این کامپیوتر در ۲۴ ساعت گذشته است (نه شمارنده‌ی رسمی گوگل). "
                "حدها مربوط به پلن رایگان‌اند و سهمیه معمولاً به ازای هر پروژه (\u200eper-project\u200f) محاسبه می‌شود.")
        tk.Label(body, text=note, bg=t["bg"], fg=t["fg_dim"], font=("Segoe UI", 8),
                 wraplength=640, justify="right", anchor="e").pack(fill="x")

        box.bind("<Double-Button-1>", lambda _e: activate())
        win.bind("<Return>", lambda _e: add())
        win.bind("<Escape>", lambda _e: win.destroy())
        refresh()
        _center_over(win, self.root)

    def set_model(self, model):
        """مدل فعال را عوض میکند و فوراً ذخیره میکند (سهمیهی هر مدل جداست)."""
        if model not in MODELS:
            return
        self.cfg["model"] = model
        self.model_pick.set(model)
        self.model_var.set(MODELS[model][1] + " ▾")
        save_config(self.cfg)
        if self.last_text and not self.last_ok:
            self._translate_text(self.last_text)    # همان متن، این بار با مدل تازه
        else:
            self.last_text = None                   # انتخاب دوباره = ترجمه با مدل تازه
            self.status_var.set("مدل: " + MODELS[model][2])

    def _keep_topmost(self):
        try:
            self.root.attributes("-topmost", True)
            for child in self.root.winfo_children():
                if isinstance(child, tk.Toplevel) and child.winfo_exists():
                    child.attributes("-topmost", True)
                    child.lift()
        except tk.TclError:
            return
        self.root.after(4000, self._keep_topmost)

    # ---- settings
    def open_settings(self):
        cfg = self.cfg
        t = self._theme()
        win = tk.Toplevel(self.root)
        win.title("تنظیمات")
        win.configure(bg=t["bg"])
        win.attributes("-topmost", True)
        win.resizable(False, False)
        key_var = tk.StringVar(value=cfg["api_key"])
        mod_var = tk.StringVar(value=MODELS[cfg["model"]][2])
        tmo_var = tk.StringVar(value=str(cfg["timeout"]))
        rty_var = tk.StringVar(value=str(cfg["retries"]))
        tgt_var = tk.StringVar(value=cfg["target"])
        proxy_var = tk.StringVar(value=cfg["proxy"])

        key_wrap = tk.Frame(win, bg=t["bg"])
        key_entry = _themed_entry(key_wrap, t, key_var, width=38, show="•")
        key_entry.pack(side="left")
        shown_key = [False]

        def toggle_key_vis():
            shown_key[0] = not shown_key[0]
            key_entry.configure(show="" if shown_key[0] else "•")
            eye_btn.set_icon("eye_off" if shown_key[0] else "eye")
            eye_btn.set_fg_key("accent" if shown_key[0] else "fg_muted", "fg")

        eye_btn = VectorButton(key_wrap, "eye", toggle_key_vis, self._theme,
                               bg_key="card_bg", hover_bg_key="hover_bg",
                               size=13, width=24, height=22)
        eye_btn.pack(side="left", padx=(4, 0))

        mod_btn = tk.Menubutton(win, textvariable=mod_var, bd=0, bg=t["card_bg"], fg=t["fg"],
                                activebackground=t["hover_bg"], activeforeground=t["fg"],
                                font=("Segoe UI", 9), indicatoron=False, padx=10, pady=3,
                                highlightthickness=1, highlightbackground=t["border"], cursor="hand2")
        mod_menu = tk.Menu(mod_btn, tearoff=0)
        _style_menu(mod_menu, t)
        for label in LABEL_OF:
            mod_menu.add_radiobutton(label=label, value=label, variable=mod_var)
        mod_btn.configure(menu=mod_menu)
        _bind_hover(mod_btn, self._theme, "card_bg", "hover_bg")

        rows = [("کلید Google API", key_wrap),
                ("مدل", mod_btn),
                ("زبان مقصد", _themed_entry(win, t, tgt_var, width=10)),
                ("پروکسی (اختیاری)", _themed_entry(win, t, proxy_var, width=30)),
                ("مهلت پاسخ (ثانیه)", _themed_entry(win, t, tmo_var, width=8)),
                ("تلاش مجدد", _themed_entry(win, t, rty_var, width=8))]
        for i, (label, widget) in enumerate(rows):
            tk.Label(win, text=label, bg=t["bg"], fg=t["fg"],
                     font=("Segoe UI", 9)).grid(row=i, column=0, sticky="e", padx=10, pady=6)
            widget.grid(row=i, column=1, sticky="w", padx=10, pady=6)
        tk.Label(win, text="هر مدل سهمیه‌ی روزانه‌ی جدا دارد؛ اگر یکی پر شد، مدل دیگری را انتخاب کنید.\n"
                           "پروکسی: اگر گوگل دسترسی را رد کرد، مثل http://127.0.0.1:10808",
                 bg=t["bg"], fg=t["fg_dim"], font=("Segoe UI", 8), justify="left").grid(
                     row=6, column=0, columnspan=2, padx=10, pady=(0, 6))

        test_var = tk.StringVar(value="")
        box = {}

        def current():
            return {"model": LABEL_OF.get(mod_var.get(), DEFAULT_MODEL),
                    "api_key": key_var.get().strip(),
                    "target": (tgt_var.get().strip() or "fa").lower(),
                    "proxy": proxy_var.get().strip(),
                    "timeout": _int(tmo_var.get(), DEFAULTS["timeout"], 5, 120),
                    "retries": _int(rty_var.get(), DEFAULTS["retries"], 0, 5)}

        def test():
            test_var.set("در حال تست…")
            box.clear()
            tried = current()               # مقادیر باید در ترد اصلی خوانده شوند

            def work():
                try:
                    _src, out = translate("Hello, this is a connection test.", tried)
                    box["msg"] = "✓ اتصال درست است — " + (out or "")[:40]
                except Exception as exc:            # noqa: BLE001
                    box["msg"] = "✗ " + str(exc)[:200]

            threading.Thread(target=work, daemon=True).start()

            def watch():                            # نتیجه از ترد، امن به UI
                if "msg" in box:
                    test_var.set(box["msg"])
                else:
                    win.after(150, watch)

            win.after(150, watch)

        def save():
            cfg["model"] = LABEL_OF.get(mod_var.get(), DEFAULT_MODEL)
            cfg["api_key"] = key_var.get().strip()
            if cfg["api_key"] and cfg["api_key"] not in cfg["api_keys"]:
                cfg["api_keys"].append(cfg["api_key"])
            cfg["target"] = (tgt_var.get().strip() or "fa").lower()
            cfg["proxy"] = proxy_var.get().strip()
            cfg["timeout"] = _int(tmo_var.get(), DEFAULTS["timeout"], 5, 120)
            cfg["retries"] = _int(rty_var.get(), DEFAULTS["retries"], 0, 5)
            save_config(cfg)
            self.model_pick.set(cfg["model"])
            self.model_var.set(MODELS[cfg["model"]][1] + " ▾")
            self._refresh_keys()
            self._apply_dir()
            self.last_text = None
            self.status_var.set("ذخیره شد")
            win.destroy()

        bar = tk.Frame(win, bg=t["bg"])
        bar.grid(row=7, column=0, columnspan=2, pady=(4, 0))
        test_btn = tk.Button(bar, text="تست اتصال", command=test, bg=t["btn_bg"], fg=t["fg"],
                             bd=0, padx=12, pady=5, cursor="hand2", font=("Segoe UI", 9))
        test_btn.pack(side="left", padx=6)
        _bind_hover(test_btn, self._theme, "btn_bg", "hover_bg")
        save_btn = tk.Button(bar, text="ذخیره", command=save, bg=t["accent"], fg=t["accent_fg"],
                             bd=0, padx=18, pady=5, cursor="hand2", font=("Segoe UI", 9, "bold"))
        save_btn.pack(side="left", padx=6)
        _bind_hover(save_btn, self._theme, "accent", "accent_hover")
        cancel_btn = tk.Button(bar, text="بستن", command=win.destroy, bg=t["btn_bg"], fg=t["fg_muted"],
                               bd=0, padx=12, pady=5, cursor="hand2", font=("Segoe UI", 9))
        cancel_btn.pack(side="left", padx=6)
        _bind_hover(cancel_btn, self._theme, "btn_bg", "hover_bg")
        tk.Label(win, textvariable=test_var, bg=t["bg"], fg=t["fg_muted"], font=("Segoe UI", 8),
                 wraplength=340, justify="left").grid(row=8, column=0, columnspan=2, pady=(6, 10))
        win.bind("<Return>", lambda _e: save())
        win.bind("<Escape>", lambda _e: win.destroy())
        _center_over(win, self.root)

    def _rtl(self):
        """جهت متن ترجمه: اگر کاربر دستی سوییچ کرده باشد همان، وگرنه خودکار از زبان مقصد."""
        rtl = self.cfg.get("rtl")
        if rtl is None:
            rtl = self.cfg["target"].split("-")[0] in RTL_LANGS
        return bool(rtl)

    def _apply_dir(self):
        t = self._theme()
        rtl = self._rtl()
        self.rtl_var.set("RTL" if rtl else "LTR")
        self.rtl_btn.configure(fg=t["accent"] if rtl else t["fg_muted"])
        # هر دو تگ را همیشه پیکربندی می‌کنیم؛ اگر فقط یکی را تنظیم کنیم، تگ دیگر
        # تراز پیش‌فرض Tk را دارد.
        self.out.tag_configure("rtl", justify="right", lmargin1=0, rmargin=0)
        self.out.tag_configure("ltr", justify="left", lmargin1=0, rmargin=0)

    def toggle_rtl(self):
        self.cfg["rtl"] = not self._rtl()
        save_config(self.cfg)
        self._apply_dir()
        if self.last_out is not None:               # متن فعلی همان لحظه جابهجا شود
            self._set_out(self.last_out, self.last_tag)
        self.status_var.set("جهت متن: " + ("راست‌به‌چپ" if self.cfg["rtl"] else "چپ‌به‌راست"))

    # ---- event pump (Tk thread only)
    def _pump(self):
        try:
            while True:
                self._handle(self.events.get_nowait())
        except queue.Empty:
            pass
        self.root.after(POLL_MS, self._pump)

    def _handle(self, ev):
        kind = ev[0]
        if kind == "result":
            _, seq, text, out, err = ev
            if seq != self.seq:
                return                                  # نتیجه‌ی قدیمی
            if err:
                log("translate error: " + err)
                self.last_ok = False
                self._set_out(err, "err")
                self.status_var.set("ناموفق")
                self.spinner.set_state("err")
                self.copy_btn.set_fg_key("fg_muted", "fg")
            else:
                self.last_ok = True
                self._set_out(out)
                self.spinner.set_state("ok")
                self.copy_btn.set_fg_key("accent", "fg")
                if self.cfg.get("autocopy") and set_clipboard(out):
                    self.status_var.set("ترجمه شد و کپی شد ✓")
                else:
                    self.status_var.set("ترجمه شد")

    def _from_clipboard(self):
        """اگر Ctrl+C خودکار جواب نداد: خودت کپی کن، بعد این را بزن."""
        text = clipboard_text()
        if not text.strip():
            self.status_var.set("کلیپ‌بورد خالی است")
            return
        self.last_text = None
        self._translate_text(text)

    # ---- copy the translation
    def copy_translation(self, _event=None):
        """کل آخرین ترجمه‌ی موفق را در کلیپ‌بورد می‌گذارد (دکمه/منو/Ctrl+Shift+C)."""
        if not self.last_ok or not self.last_out:
            self.status_var.set("ترجمه‌ای برای کپی نیست")
            return "break"
        if not set_clipboard(self.last_out):
            self.status_var.set("کپی ناموفق بود — کلیپ‌بورد قفل است؟")
            return "break"
        self.status_var.set("ترجمه کپی شد ✓")
        self.copy_btn.set_icon("check")
        self.copy_btn.set_fg_key("accent_green", "accent_green")

        def _restore_copy():
            self.copy_btn.set_icon("copy")
            self.copy_btn.set_fg_key("accent" if self.last_ok else "fg_muted", "fg")

        self.root.after(1500, _restore_copy)
        return "break"

    def _copy_out(self, _event=None):
        """Ctrl+C روی خود پنجره: منطقی‌ترین متنِ خطوط انتخاب‌شده را کپی می‌کند."""
        try:
            first = int(self.out.index("sel.first").split(".")[0]) - 1
            last = int(self.out.index("sel.last").split(".")[0]) - 1
            sel = " ".join(self._layout_source[first:last + 1]).strip()
        except (tk.TclError, ValueError, IndexError):
            sel = None
        if sel:
            self.status_var.set("کپی شد ✓" if set_clipboard(sel) else "کپی ناموفق بود")
            return "break"
        return self.copy_translation()

    def _set_autocopy(self, val):
        self.autocopy_var.set(bool(val))
        self.cfg["autocopy"] = bool(val)
        save_config(self.cfg)
        self.status_var.set("کپی خودکار: " + ("روشن" if self.cfg["autocopy"] else "خاموش"))

    def toggle_autocopy(self):
        self._set_autocopy(not self.cfg.get("autocopy"))

    def _set_watch(self, val):
        self.watch_var.set(bool(val))
        self.cfg["watch"] = bool(val)
        self.watcher.enabled = self.cfg["watch"]
        self.watch_badge_var.set("" if self.cfg["watch"] else "ترجمه خودکار خاموش")
        save_config(self.cfg)
        self.status_var.set("ترجمه‌ی خودکار: "
                            + ("روشن" if self.cfg["watch"] else "خاموش"))

    def toggle_watch(self):
        self._set_watch(not self.cfg.get("watch", True))

    def _grab_selection(self):
        log("selection gesture -> Ctrl+C")
        # شماره‌ی توالی کلیپ‌بورد: اگر بعد از Ctrl+C عوض نشد، یعنی کپی انجام نشده
        self.clip_seq = user32.GetClipboardSequenceNumber()
        send_ctrl_c()
        self.root.after(120, send_ctrl_c)           # یک بار دیگر، اگر اولی گم شد
        self.root.after(COPY_SETTLE_MS + 120, self._read_clipboard)

    def _read_clipboard(self, retry=True):
        if user32.GetClipboardSequenceNumber() == self.clip_seq:
            if retry:                               # برنامه‌های سنگین دیر کپی می‌کنند
                self.root.after(200, lambda: self._read_clipboard(False))
            else:
                log("clipboard sequence unchanged -> copy never happened")
                self.status_var.set("متن کپی نشد — دوباره انتخاب کنید")
            return
        self._translate_text(clipboard_text())

    def _translate_text(self, text):
        text = " ".join(str(text or "").split())[:MAX_CHARS]
        if not text or (text == self.last_text and self.last_ok):
            return
        log("clipboard: %d chars" % len(text))
        self.last_text = text
        self.last_ok = False
        self.seq += 1
        self._set_source(text)
        self.status_var.set("در حال ترجمه…")
        self.spinner.set_state("loading")
        self._set_out("")
        threading.Thread(target=self._work, args=(self.seq, text), daemon=True).start()

    def _work(self, seq, text):
        try:
            _src, out = translate(text, self.cfg)
            self.events.put(("result", seq, text, out, None))
        except Exception as exc:                        # noqa: BLE001
            self.events.put(("result", seq, text, None, str(exc)))

    def _set_out(self, text, tag=None):
        self.last_out, self.last_tag = text, tag
        rtl = self._rtl()
        width = self.out.winfo_width()
        if width <= 1:                              # هنوز نقشه نشده
            width = WIDTH - 32
        width -= 2 * int(self.out.cget("padx"))
        shown, self._layout_source = _visual_layout(text, self.out_font, width, rtl)
        tags = ([tag] if tag else []) + [("rtl" if rtl else "ltr")]
        self.out.configure(state="normal")
        self.out.delete("1.0", "end")
        self.out.insert("1.0", shown, tuple(tags))
        self.out.configure(state="disabled")


# ---------------------------------------------------------------- selftest
def selftest():
    global USAGE_PATH
    USAGE_PATH = os.path.join(APP_DIR, "_selftest_usage.json")
    if os.path.exists(USAGE_PATH):
        os.remove(USAGE_PATH)          # اجرای دوباره نباید مصرف اجرای قبلی را بشمارد
    assert parse_google({"data": {"translations": [
        {"translatedText": "a &amp; b", "detectedSourceLanguage": "en"}]}}) == ("en", "a & b")
    assert parse_google({"data": {"translations": [{"translatedText": "x"}]}}) == ("", "x")
    # کشیدن = انتخاب
    assert is_select_gesture((100, 100), (90, 90), None, 0, 0) is True
    # کلیک ساده = نه
    assert is_select_gesture((100, 100), (99, 99), None, 0, 0) is False
    # دوبار کلیک سریع روی یک نقطه = انتخاب کلمه
    assert is_select_gesture((100, 100), (100, 100), (100, 101), 100.0, 100.2) is True
    # دو کلیک با فاصله‌ی زمانی زیاد = نه
    assert is_select_gesture((100, 100), (100, 100), (100, 101), 100.0, 102.0) is False
    # دو کلیک در دو نقطه‌ی دور = نه
    assert is_select_gesture((300, 300), (300, 300), (100, 100), 100.0, 100.2) is False
    p = os.path.join(APP_DIR, "_selftest_config.json")
    try:
        cfg = load_config(p)
        assert cfg["target"] == "fa" and cfg["model"] == DEFAULT_MODEL
        assert cfg["timeout"] == 20 and cfg["retries"] == 2
        cfg["x"], cfg["api_key"], cfg["rtl"] = 321, "k", True
        save_config(cfg, p)
        assert load_config(p)["x"] == 321 and load_config(p)["api_key"] == "k"
        assert load_config(p)["rtl"] is True
        with open(p, "w", encoding="utf-8") as f:    # config قدیمی (engine/gemini_model)
            json.dump({"engine": "google", "api_key": "k"}, f)
        assert load_config(p)["model"] == "google-translate"
        with open(p, "w", encoding="utf-8") as f:
            json.dump({"api_key": "a", "api_keys": ["b", "a", "b"]}, f)
        migrated = load_config(p)
        assert migrated["api_keys"] == ["b", "a"] and migrated["api_key"] == "a"
    finally:
        if os.path.exists(p):
            os.remove(p)
    record_usage("key-a", "gemini-2.5-flash", {"promptTokenCount": 100}, now=1000)
    record_usage("key-a", "gemini-2.5-flash", {"promptTokenCount": 150}, now=1010)
    row = next(r for r in usage_rows(["key-a"], now=1020) if r[1] == "gemini-2.5-flash")
    assert row[2:6] == (2, 250, 2, (5, 250000, 20)), row
    assert _proxy_url({"proxy": "127.0.0.1:10809/"}) == "http://127.0.0.1:10809"
    assert _proxy_url({}) == ""
    # جهت متن: خودکار از زبان مقصد، مگر اینکه کاربر دستی انتخاب کرده باشد
    class _Cfg:
        def __init__(self, cfg):
            self.cfg = cfg

    assert Overlay._rtl(_Cfg({"target": "fa", "rtl": None})) is True
    assert Overlay._rtl(_Cfg({"target": "en", "rtl": None})) is False
    assert Overlay._rtl(_Cfg({"target": "fa", "rtl": False})) is False
    assert Overlay._rtl(_Cfg({"target": "en", "rtl": True})) is True
    # چیدمان BiDi: ترتیب اجراها عوض می‌شود، ولی عبارت انگلیسی و عدد از هم نمی‌پاشند.
    units = ["مصنوعی", "عاملیت‌محور", "(Agentic", "AI)", "می‌رسند؛"]
    assert _visual_line(units, True) == "می‌رسند؛ (Agentic AI) مصنوعی عاملیت‌محور"
    assert _visual_line(units, False) == "مصنوعی عاملیت‌محور (Agentic AI) می‌رسند؛"
    units = ["در", "سال", "2024،", "Salesforce", "Agentforce", "معرفی", "شد."]
    assert _visual_line(units, True) == "معرفی شد. 2024، Salesforce Agentforce در سال"

    class _Measure:
        def measure(self, text):
            return len(text)

    shown, source = _visual_layout("one two three four", _Measure(), 7, False)
    assert shown == "one two\nthree\nfour", shown
    shown, source = _visual_layout("الف ببب پپپ", _Measure(), 10, True)
    assert shown == "الف ببب\nپپپ", shown
    assert source == ["الف ببب", "پپپ"], source
    # پاک‌سازی متن انتخاب‌شده (شکستن خط و فاصله‌های تکراری)
    assert " ".join("a\n b\tc ".split()) == "a b c"

    # ماشین حالت پایش ماوس: کشیدن = انتخاب، کلیک ساده = نه، دوبار کلیک = بله
    global is_our_window
    real_is_ours = is_our_window
    is_our_window = lambda pt: False                # noqa: E731 - برای تست
    try:
        hits = []
        mp = MousePoller(lambda: hits.append(1))
        mp.feed((100, 100), True, 10.0)
        mp.feed((220, 100), False, 10.3)            # کشیدن ۱۲۰ پیکسل
        assert hits == [1], hits
        mp.feed((100, 100), True, 11.0)
        mp.feed((101, 100), False, 11.05)           # کلیک ساده
        assert hits == [1], hits
        mp.feed((100, 100), True, 12.0)
        mp.feed((100, 101), False, 12.1)            # کلیک اول
        mp.feed((100, 100), True, 12.2)
        mp.feed((100, 101), False, 12.3)            # کلیک دوم همان نقطه = انتخاب کلمه
        assert hits == [1, 1], hits
        mp.enabled = False                          # خاموش‌کردن از منوی راست‌کلیک
        mp.feed((100, 100), True, 14.0)
        mp.feed((220, 100), False, 14.3)            # همان کشیدن، بی‌اثر
        assert hits == [1, 1], hits
        mp.enabled = True
        mp.feed((100, 100), True, 15.0)
        mp.feed((220, 100), False, 15.3)
        assert hits == [1, 1, 1], hits
        is_our_window = lambda pt: True             # کشیدن از روی پنل خودمان
        mp.feed((100, 100), True, 13.0)
        mp.feed((220, 100), False, 13.3)
        assert hits == [1, 1, 1], hits
    finally:
        is_our_window = real_is_ours

    # مسیر کامل ترجمه با پاسخ ساختگی (بدون تماس واقعی با API)
    class _Fake:
        def __init__(self, payload):
            self.body = json.dumps(payload).encode()

        def read(self):
            return self.body

        def __enter__(self):
            return self

        def __exit__(self, *_a):
            return False

    calls = []

    def fake_urlopen(req, timeout=0):
        calls.append(req.full_url)
        if "generativelanguage" in req.full_url:
            return _Fake({"candidates": [{"content": {"parts": [{"text": " سلام "}]}}]})
        body = json.loads(req.data)
        if body["target"] == "fa" and body["q"][0].startswith("سلام"):
            return _Fake({"data": {"translations": [
                {"translatedText": "همان", "detectedSourceLanguage": "fa"}]}})
        return _Fake({"data": {"translations": [
            {"translatedText": "translated &amp; ok", "detectedSourceLanguage": "en"}]}})

    google = {"model": "google-translate", "api_key": "k", "target": "fa"}
    real = urllib.request.urlopen
    urllib.request.urlopen = fake_urlopen
    try:
        assert translate("hi", google) == ("en", "translated & ok")
        n = len(calls)
        # متن فارسی با مقصد فارسی → یک بار دیگر به انگلیسی ترجمه می‌شود
        assert translate("سلام", google) == ("en", "translated & ok")
        assert len(calls) == n + 2 and "translation.googleapis.com" in calls[-1]
        assert translate("x", {"model": "gemini-3.7-flash", "api_key": "k", "target": "fa"}) \
            == ("", "سلام")
        assert "gemini-3.7-flash" in calls[-1]
        try:
            translate("x", {"model": "google-translate", "api_key": " ", "target": "fa"})
            raise AssertionError("کلید خالی باید خطا بدهد")
        except RuntimeError:
            pass
    finally:
        urllib.request.urlopen = real

    # دو خطای گذرای شبکه و بعد موفقیت → باید دو بار تلاش مجدد کند
    attempts = []

    def flaky(req, timeout=0):
        attempts.append(timeout)
        if len(attempts) < 3:
            raise urllib.error.URLError(TimeoutError("timed out"))
        return _Fake({"data": {"translations": [
            {"translatedText": "ok", "detectedSourceLanguage": "en"}]}})

    real_sleep, real = time.sleep, urllib.request.urlopen
    time.sleep = lambda _s: None
    urllib.request.urlopen = flaky
    try:
        assert translate("hi", {"model": "google-translate", "api_key": "k", "target": "fa",
                                "timeout": 7, "retries": 2}) == ("en", "ok")
        assert attempts == [7, 7, 7], attempts
    finally:
        time.sleep, urllib.request.urlopen = real_sleep, real

    # خطای دائمی (403) نباید تکرار شود
    attempts.clear()

    def forbidden(req, timeout=0):
        attempts.append(timeout)
        raise urllib.error.HTTPError(req.full_url, 403, "Forbidden", {}, None)

    urllib.request.urlopen = forbidden
    try:
        try:
            translate("hi", {"model": "google-translate", "api_key": "k", "target": "fa",
                             "timeout": 5, "retries": 2})
            raise AssertionError("403 باید خطا بدهد")
        except RuntimeError as exc:
            assert "403" in str(exc), exc
        assert len(attempts) == 1, attempts
    finally:
        urllib.request.urlopen = real
    try:
        os.remove(USAGE_PATH)
    except OSError:
        pass
    print("selftest: ok")


# ---------------------------------------------------------------- main
def main():
    cfg = load_config()
    root = tk.Tk()
    app = Overlay(root, cfg)
    app._apply_dir()
    root.mainloop()


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        try:
            main()
        except Exception:                 # pythonw کنسول ندارد؛ خطا را ذخیره کن
            import traceback
            with open(os.path.join(APP_DIR, "error.log"), "w", encoding="utf-8") as f:
                traceback.print_exc(file=f)
            raise
