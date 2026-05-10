"""Mac 剪贴板读写模块，支持文本和图片。"""

import base64
import hashlib
import io
import subprocess

from PIL import Image

# 延迟导入 AppKit（仅在 macOS 上可用）
_NSPasteboard = None
_NSPasteboardTypePNG = None
_NSPasteboardTypeString = None
_NSData = None
_NSImage = None


def _ensure_appkit():
    global _NSPasteboard, _NSPasteboardTypePNG, _NSPasteboardTypeString, _NSData, _NSImage
    if _NSPasteboard is not None:
        return
    from AppKit import NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString, NSData, NSImage
    _NSPasteboard = NSPasteboard
    _NSPasteboardTypePNG = NSPasteboardTypePNG
    _NSPasteboardTypeString = NSPasteboardTypeString
    _NSData = NSData
    _NSImage = NSImage


def get_text() -> str | None:
    """从剪贴板获取文本。"""
    try:
        result = subprocess.run(
            ["pbpaste"], capture_output=True, text=True, timeout=5
        )
        if result.returncode == 0 and result.stdout:
            return result.stdout
    except Exception:
        pass
    return None


def set_text(text: str) -> bool:
    """将文本写入剪贴板。"""
    try:
        process = subprocess.Popen(
            ["pbcopy"], stdin=subprocess.PIPE
        )
        process.communicate(text.encode("utf-8"))
        return process.returncode == 0
    except Exception:
        return False


def get_image() -> str | None:
    """从剪贴板获取图片，返回 base64 编码的 PNG 数据。"""
    try:
        _ensure_appkit()
        pb = _NSPasteboard.generalPasteboard()
        data = pb.dataForType_(_NSPasteboardTypePNG)
        if data is None:
            # 尝试 TIFF 格式
            from AppKit import NSPasteboardTypeTIFF
            tiff_data = pb.dataForType_(NSPasteboardTypeTIFF)
            if tiff_data is None:
                return None
            # 将 TIFF 转为 PNG
            image = _NSImage.alloc().initWithData_(tiff_data)
            if image is None:
                return None
            tiff_rep = image.TIFFRepresentation()
            from AppKit import NSBitmapImageRep
            bitmap = NSBitmapImageRep.imageRepWithData_(tiff_rep)
            from AppKit import NSPNGFileType
            data = bitmap.representationUsingType_properties_(NSPNGFileType, None)

        if data:
            raw_bytes = bytes(data)
            return base64.b64encode(raw_bytes).decode("ascii")
    except Exception:
        pass
    return None


def set_image(base64_data: str) -> bool:
    """将 base64 编码的 PNG 图片写入剪贴板。"""
    try:
        _ensure_appkit()
        raw_bytes = base64.b64decode(base64_data)
        pb = _NSPasteboard.generalPasteboard()
        pb.clearContents()
        ns_data = _NSData.dataWithBytes_length_(raw_bytes, len(raw_bytes))
        pb.setData_forType_(ns_data, _NSPasteboardTypePNG)
        return True
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
    # 先检查图片（优先级更高）
    image_data = get_image()
    if image_data:
        return ("image", image_data)

    text = get_text()
    if text:
        return ("text", text)

    return ("empty", None)


def get_clipboard_hash() -> str:
    """返回当前剪贴板内容的 hash，用于检测变化。"""
    _ensure_appkit()
    pb = _NSPasteboard.generalPasteboard()
    # macOS 的 changeCount 在每次剪贴板变化时递增
    return str(pb.changeCount())
