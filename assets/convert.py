import os
from PySide6.QtGui import QImage, QPainter
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtCore import Qt

src_dir = r"c:\Users\papop\Desktop\TaskScheduler\MicMute\src\MicMute\assets"
dst_dir = r"c:\Users\papop\Desktop\TaskScheduler\MicMuteRs\assets"

os.makedirs(dst_dir, exist_ok=True)

for file in os.listdir(src_dir):
    if file.endswith(".svg"):
        svg_path = os.path.join(src_dir, file)
        png_path = os.path.join(dst_dir, file.replace(".svg", ".png"))
        
        renderer = QSvgRenderer(svg_path)
        img = QImage(64, 64, QImage.Format_ARGB32)
        img.fill(Qt.transparent)
        
        painter = QPainter(img)
        painter.setRenderHint(QPainter.Antialiasing)
        renderer.render(painter)
        painter.end()
        
        img.save(png_path)

print("Icons converted successfully.")
