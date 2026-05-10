"""Windows 剪贴板读写模块，支持文本和图片。"""

import base64
import io
import ctypes
import time

from PIL import Image

# 延迟导入 win32clipboard（仅在 Windows 上可用）
_win32clipboard = None
_CF_TEXT = 1
_CF_UNICODETEXT = 13
_CF_DIB = 8
_CF_DIBV5 = 17


def _ensure_win32():
    global _win32clipboard
    if _win32clipboard is not None:
        return
    import win32clipboard
    _win32clipboard = win32clipboard


def _open_clipboard_retry(max_retries=5, delay=0.1):
    """尝试打开剪贴板，失败时重试（其他程序可能正在使用）。"""
    _ensure_win32()
    for i in range(max_retries):
        try:
            _win32clipboard.OpenClipboard()
            return True
        except Exception:
            if i < max_retries - 1:
                time.sleep(delay)
    return False


def get_text() -> str | None:
    """从剪贴板获取文本。"""
    try:
        _ensure_win32()
        if not _open_clipboard_retry():
            return None
        try:
            data = _win32clipboard.GetClipboardData(_CF_UNICODETEXT)
            return data if data else None
        except TypeError:
            return None
        finally:
            _win32clipboard.CloseClipboard()
    except Exception:
        return None


def set_text(text: str) -> bool:
    """将文本写入剪贴板。"""
    try:
        _ensure_win32()
        if not _open_clipboard_retry():
            return False
        try:
            _win32clipboard.EmptyClipboard()
            _win32clipboard.SetClipboardData(_CF_UNICODETEXT, text)
            return True
        finally:
            _win32clipboard.CloseClipboard()
    except Exception:
        return False


def get_image() -> str | None:
    """从剪贴板获取图片，返回 base64 编码的 PNG 数据。"""
    try:
        _ensure_win32()
        if not _open_clipboard_retry():
            return None
        try:
            # 尝试获取 DIB 格式数据
            if not _win32clipboard.IsClipboardFormatAvailable(_CF_DIB):
                return None
            data = _win32clipboard.GetClipboardData(_CF_DIB)
            if not data:
                return None
        finally:
            _win32clipboard.CloseClipboard()

        # 解析 BITMAPINFOHEADER 获取尺寸信息
        # DIB 数据 = BITMAPINFOHEADER + 像素数据
        header_size = int.from_bytes(data[0:4], "little")
        width = int.from_bytes(data[4:8], "little", signed=True)
        height = int.from_bytes(data[8:12], "little", signed=True)
        bits_per_pixel = int.from_bytes(data[14:16], "little")
        compression = int.from_bytes(data[16:20], "little")

        # 构建完整的 BMP 文件
        # BMP 文件头 = 14 bytes
        bmp_file_size = 14 + len(data)
        pixel_offset = 14 + header_size
        if bits_per_pixel <= 8:
            # 有颜色表
            num_colors = 1 << bits_per_pixel
            pixel_offset += num_colors * 4

        bmp_header = b"BM"
        bmp_header += bmp_file_size.to_bytes(4, "little")
        bmp_header += b"\x00\x00\x00\x00"  # reserved
        bmp_header += pixel_offset.to_bytes(4, "little")

        bmp_data = bmp_header + data

        # 用 Pillow 转换为 PNG
        img = Image.open(io.BytesIO(bmp_data))
        png_buffer = io.BytesIO()
        img.save(png_buffer, format="PNG")
        return base64.b64encode(png_buffer.getvalue()).decode("ascii")
    except Exception:
        return None


def set_image(base64_data: str) -> bool:
    """将 base64 编码的 PNG 图片写入剪贴板。"""
    try:
        _ensure_win32()
        raw_bytes = base64.b64decode(base64_data)
        img = Image.open(io.BytesIO(raw_bytes))

        # 转换为 BMP 格式
        bmp_buffer = io.BytesIO()
        # 确保是 RGB 模式（去掉 alpha 通道）
        if img.mode == "RGBA":
            bg = Image.new("RGB", img.size, (255, 255, 255))
            bg.paste(img, mask=img.split()[3])
            img = bg
        elif img.mode != "RGB":
            img = img.convert("RGB")

        img.save(bmp_buffer, format="BMP")
        bmp_data = bmp_buffer.getvalue()

        # 去掉 BMP 文件头（14 bytes），只保留 DIB 数据
        dib_data = bmp_data[14:]

        if not _open_clipboard_retry():
            return False
        try:
            _win32clipboard.EmptyClipboard()
            _win32clipboard.SetClipboardData(_CF_DIB, dib_data)
            return True
        finally:
            _win32clipboard.CloseClipboard()
    except Exception:
        return False


def get_clipboard_content() -> tuple[str, str | None]:
    """
    获取当前剪贴板内容。
    返回 (content_type, content)：
    - ("text", text_content) 文本
    - ("image", base64_data) 图片
    - ("empty", None) 空剪贴板
    """
    # 先检查图片
    image_data = get_image()
    if image_data:
        return ("image", image_data)

    text = get_text()
    if text:
        return ("text", text)

    return ("empty", None)


def get_clipboard_hash() -> str:
    """返回当前剪贴板内容的序列号，用于检测变化。"""
    try:
        # 使用 Windows API GetClipboardSequenceNumber
        return str(ctypes.windll.user32.GetClipboardSequenceNumber())
    except Exception:
        return ""
