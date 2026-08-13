"""
PDF 转换处理器 — 统一入口
用法: python pdf_handler.py <operation> <input_path> <output_path>

操作列表:
  pdf_to_txt          PDF → 纯文本（PyMuPDF 逐页）
  pdf_to_html         PDF → HTML（PyMuPDF 样式 + pdfplumber 表格）
  pdf_to_md           PDF → Markdown（PyMuPDF 样式 + pdfplumber 表格）
  pdf_to_docx         PDF → DOCX（pdf2docx）
  docx_to_pdf         DOCX → PDF（python-docx + reportlab）
  md_to_pdf           Markdown → PDF（markdown + WeasyPrint）
  html_to_pdf         HTML → PDF（WeasyPrint）
  txt_to_pdf          纯文本 → PDF（reportlab）
  pdf_to_html_light   PDF → HTML（仅纯文本，超大文件降级）
  pdf_to_md_light     PDF → Markdown（仅纯文本，超大文件降级）

协议：
  成功: 输出 OK:<filesize> 到 stdout
  失败: 输出 ERROR:<message> 到 stderr，退出码 1
"""

import sys
import os

# stdout/stderr 强制 UTF-8：Windows 默认 ANSI 代码页（中文系统为 GBK），
# 而 Rust 侧按 UTF-8 解码子进程输出，不强制就会乱码（双保险，Rust 侧同时设 PYTHONIOENCODING）
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8")
    except Exception:
        pass

# ============================================================
# 内存监控（上限由 main() 按文件体积 + 设备内存 + 并发数自适应计算）
# ============================================================

# 内存上限（MB）：默认 2GB，main() 中按源文件体积与设备实际内存自适应
_MEMORY_LIMIT_MB = 2048.0


def _compute_memory_limit(file_mb: float) -> float:
    """计算内存上限（MB）：多重约束取最小 + 保底，绝不脱离设备实际内存

    约束（取最小）：
    1. 按文件体积的需求量：基础 2GB + 每 1MB 文件追加 8MB，封顶 12GB
    2. Rust 侧按设备物理内存下发的每进程预算（环境变量 CONVERT_MEM_LIMIT_MB，
       为物理内存 × 3/4 ÷ 并发数，保证多进程并发时不挤爆设备）
    3. 环境变量缺失时兜底：psutil 探测物理内存 × 3/4
    最后保底 512MB：小内存机器也至少能转小文件
    """
    need = min(12288.0, 2048.0 + file_mb * 8)

    budget = None
    budget_env = os.environ.get("CONVERT_MEM_LIMIT_MB")
    if budget_env:
        try:
            budget = float(budget_env)
        except ValueError:
            budget = None
    if budget is None:
        try:
            import psutil
            budget = psutil.virtual_memory().total / 1024 / 1024 * 0.75
        except Exception:
            budget = None

    limit = need if budget is None else min(need, budget)
    return max(512.0, limit)


def check_memory():
    """检查当前进程内存，超过自适应上限抛出 MemoryError"""
    try:
        import psutil
        process = psutil.Process(os.getpid())
        mem_mb = process.memory_info().rss / 1024 / 1024
        if mem_mb > _MEMORY_LIMIT_MB:
            raise MemoryError(
                f"内存超限（已用 {mem_mb:.0f}MB，上限 {_MEMORY_LIMIT_MB:.0f}MB）："
                f"文件过大或内容过于复杂，请尝试拆分文件后重试"
            )
    except ImportError:
        pass  # psutil 未安装则跳过检查


def _read_text(path: str) -> str:
    """读取文本文件：编码自适应（与 Rust 侧 text_io 同策略）

    优先级：BOM 探测 → 无 BOM UTF-16 启发式 → UTF-8 → GB18030（GBK 超集）。
    UTF-16 探测必须在 GB18030 之前，否则 GB18030 会把 UTF-16 静默解成乱码。
    """
    with open(path, "rb") as f:
        raw = f.read()

    # 1. BOM 探测
    if raw.startswith(b"\xef\xbb\xbf"):
        return raw[3:].decode("utf-8")
    if raw.startswith(b"\xff\xfe"):
        return raw[2:].decode("utf-16-le")
    if raw.startswith(b"\xfe\xff"):
        return raw[2:].decode("utf-16-be")

    # 2. 无 BOM UTF-16 启发式（ASCII 范围文本：隔字节为 0x00）
    if len(raw) >= 4:
        if raw[1] == 0x00 and raw[3] == 0x00:
            return raw.decode("utf-16-le")
        if raw[0] == 0x00 and raw[2] == 0x00:
            return raw.decode("utf-16-be")

    # 3. UTF-8 严格解码 → 4. GB18030 回退
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        try:
            return raw.decode("gb18030")
        except UnicodeDecodeError:
            raise RuntimeError(
                "无法识别源文件编码：既非 UTF-8/UTF-16 也非 GB18030/GBK，请先将文件另存为 UTF-8 编码"
            )


def _xml_escape(text: str) -> str:
    """转义 XML 特殊字符（reportlab Paragraph 按 XML 解析，裸 & / < 会导致生成失败）"""
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def _get_table_bboxes(page):
    """获取页面中表格区域的边界框列表（用于跳过表格区域的重复文字）"""
    try:
        tabs = page.find_tables()
        return [tab.bbox for tab in tabs]
    except Exception:
        return []


def _in_table(bbox, table_bboxes):
    """判断文字块是否在表格区域内"""
    if not table_bboxes:
        return False
    x0, y0, x1, y1 = bbox
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    for tx0, ty0, tx1, ty1 in table_bboxes:
        if tx0 - 2 <= cx <= tx1 + 2 and ty0 - 2 <= cy <= ty1 + 2:
            return True
    return False


# ============================================================
# PDF → TXT（PyMuPDF 逐页提取纯文本）
# ============================================================

def pdf_to_txt(input_path: str, output_path: str):
    import fitz  # PyMuPDF

    lines = []
    doc = fitz.open(input_path)
    try:
        for page in doc:
            text = page.get_text("text")
            if text:
                lines.append(text.strip())
            check_memory()
    finally:
        doc.close()

    result = "\n\n".join(lines).strip()
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(result)


# ============================================================
# PDF → HTML（PyMuPDF 完整样式 + pdfplumber 表格）
# ============================================================

def pdf_to_html(input_path: str, output_path: str):
    import fitz
    from html import escape

    parts = []
    doc = fitz.open(input_path)
    # 提前打开 pdfplumber（仅一次），避免逐页重复打开引发文件冲突
    pdf_plumber = None
    try:
        import pdfplumber
        pdf_plumber = pdfplumber.open(input_path)
    except Exception:
        pass
    try:
        for page_idx, page in enumerate(doc):
            # 检测表格区域，避免文字块与表格内容重复
            table_bboxes = _get_table_bboxes(page)

            # 提取带样式的文字块（跳过表格区域内的文字）
            blocks = page.get_text("dict", flags=fitz.TEXT_PRESERVE_WHITESPACE)["blocks"]
            for block in blocks:
                if block["type"] != 0:  # 仅处理文本块
                    if block["type"] == 1:
                        parts.append('<p>【图片】</p>')
                    continue
                # 跳过表格区域内的文字块（由 pdfplumber 表格输出代替）
                if _in_table(block.get("bbox", (0, 0, 0, 0)), table_bboxes):
                    continue
                for line in block.get("lines", []):
                    line_html = ""
                    for span in line.get("spans", []):
                        text = span.get("text", "")
                        if not text.strip():
                            line_html += escape(text)
                            continue
                        styled = escape(text)
                        flags = span.get("flags", 0)
                        # 粗体
                        if flags & (1 << 4):
                            styled = f"<strong>{styled}</strong>"
                        # 斜体
                        if flags & (1 << 1):
                            styled = f"<em>{styled}</em>"
                        # 下划线
                        if flags & (1 << 5):
                            styled = f"<u>{styled}</u>"
                        # 删除线
                        if flags & (1 << 6):
                            styled = f"<del>{styled}</del>"
                        # 等宽字体
                        if flags & (1 << 3):
                            styled = f"<code>{styled}</code>"
                        # 颜色和字号
                        color = span.get("color", 0)
                        size = span.get("size", 11)
                        style_parts = []
                        if color and color != 0:
                            r = (color >> 16) & 0xFF
                            g = (color >> 8) & 0xFF
                            b = color & 0xFF
                            style_parts.append(f"color:#{r:02x}{g:02x}{b:02x}")
                        if size and abs(size - 11) > 0.5:
                            style_parts.append(f"font-size:{size:.1f}pt")
                        if style_parts:
                            styled = f'<span style="{";".join(style_parts)}">{styled}</span>'
                        line_html += styled
                    if line_html.strip():
                        parts.append(f"<p>{line_html}</p>")

            # 提取表格（复用已打开的 pdfplumber 实例，不再逐页重复打开）
            if pdf_plumber is not None:
                try:
                    if page_idx < len(pdf_plumber.pages):
                        tables = pdf_plumber.pages[page_idx].extract_tables()
                        for table in tables:
                            parts.append("<table>")
                            for i, row in enumerate(table):
                                if row is None:
                                    continue
                                tag = "th" if i == 0 else "td"
                                cells = "".join(
                                    f"<{tag}>{escape(str(c)) if c is not None else ''}</{tag}>"
                                    for c in row
                                )
                                parts.append(f"<tr>{cells}</tr>")
                            parts.append("</table>")
                except Exception:
                    pass  # 表格提取失败不阻断

            # 提取图片
            images = page.get_images()
            for img in images:
                parts.append('<p>【图片】</p>')

            check_memory()
    finally:
        doc.close()
        if pdf_plumber is not None:
            try:
                pdf_plumber.close()
            except Exception:
                pass

    title = os.path.splitext(os.path.basename(input_path))[0]
    html = (
        "<!DOCTYPE html>\n<html>\n<head>\n"
        f'<meta charset="utf-8">\n<title>{escape(title)}</title>\n'
        "<style>body{font-family:sans-serif;max-width:800px;margin:40px auto;padding:0 20px;line-height:1.8;}"
        "table{border-collapse:collapse;width:100%;}th,td{border:1px solid #ddd;padding:8px;text-align:left;}"
        "th{background:#f0f0f0;}code{background:#f5f5f5;padding:2px 4px;border-radius:3px;}</style>\n"
        "</head>\n<body>\n"
        + "\n".join(parts)
        + "\n</body>\n</html>"
    )

    with open(output_path, "w", encoding="utf-8") as f:
        f.write(html)


# ============================================================
# PDF → Markdown（PyMuPDF 样式 + pdfplumber 表格）
# ============================================================

def pdf_to_md(input_path: str, output_path: str):
    import fitz

    lines = []
    doc = fitz.open(input_path)
    # 提前打开 pdfplumber（仅一次），避免逐页重复打开引发文件冲突
    pdf_plumber = None
    try:
        import pdfplumber
        pdf_plumber = pdfplumber.open(input_path)
    except Exception:
        pass
    try:
        # 收集所有页面的字体大小，用于标题检测
        font_sizes = {}
        for page in doc:
            blocks = page.get_text("dict", flags=fitz.TEXT_PRESERVE_WHITESPACE)["blocks"]
            for block in blocks:
                if block["type"] != 0:
                    continue
                for line in block.get("lines", []):
                    for span in line.get("spans", []):
                        size = span.get("size", 0)
                        text = span.get("text", "").strip()
                        if text and size > 0:
                            if size not in font_sizes:
                                font_sizes[size] = 0
                            font_sizes[size] += len(text)
            check_memory()  # 大 PDF 字体扫描也需内存监控

        # 标题大小聚类：取前 4 个最大字号作为 h1-h4
        sorted_sizes = sorted(font_sizes.keys(), reverse=True)
        heading_thresholds = []
        base_size = sorted_sizes[0] if sorted_sizes else 11
        for s in sorted_sizes[:4]:
            if s > base_size * 1.1:
                heading_thresholds.append(s)

        for page_idx, page in enumerate(doc):
            # 检测表格区域，避免文字块与表格内容重复
            table_bboxes = _get_table_bboxes(page)

            blocks = page.get_text("dict", flags=fitz.TEXT_PRESERVE_WHITESPACE)["blocks"]
            for block in blocks:
                if block["type"] != 0:
                    if block["type"] == 1:
                        lines.append("【图片】\n")
                    continue
                # 跳过表格区域内的文字块（由 pdfplumber 表格输出代替）
                if _in_table(block.get("bbox", (0, 0, 0, 0)), table_bboxes):
                    continue
                for line_data in block.get("lines", []):
                    line_md = ""
                    is_heading = False
                    heading_level = 0
                    for span in line_data.get("spans", []):
                        text = span.get("text", "")
                        if not text:
                            continue
                        size = span.get("size", 11)
                        flags = span.get("flags", 0)

                        # 检测标题
                        for i, threshold in enumerate(heading_thresholds):
                            if size >= threshold:
                                is_heading = True
                                heading_level = i + 1
                                break

                        styled = text
                        # 粗体
                        if flags & (1 << 4):
                            styled = f"**{styled}**"
                        # 斜体
                        if flags & (1 << 1):
                            styled = f"*{styled}*"
                        # 删除线
                        if flags & (1 << 6):
                            styled = f"~~{styled}~~"
                        # 等宽
                        if flags & (1 << 3):
                            styled = f"`{styled}`"
                        line_md += styled

                    if line_md.strip():
                        if is_heading and heading_level > 0:
                            prefix = "#" * heading_level
                            lines.append(f"\n{prefix} {line_md.strip()}\n")
                        else:
                            lines.append(line_md)
                    else:
                        lines.append("")

            # 表格（复用已打开的 pdfplumber 实例）
            if pdf_plumber is not None:
                try:
                    if page_idx < len(pdf_plumber.pages):
                        tables = pdf_plumber.pages[page_idx].extract_tables()
                        for table in tables:
                            if not table:
                                continue
                            header = table[0] if table[0] else [""] * 3
                            header_cells = [str(c) if c is not None else "" for c in header]
                            lines.append("| " + " | ".join(header_cells) + " |")
                            lines.append("| " + " | ".join(["---"] * len(header_cells)) + " |")
                            for row in table[1:]:
                                if row is None:
                                    continue
                                cells = [str(c) if c is not None else "" for c in row]
                                while len(cells) < len(header_cells):
                                    cells.append("")
                                lines.append("| " + " | ".join(cells[:len(header_cells)]) + " |")
                            lines.append("")
                except Exception:
                    pass

            # 图片
            images = page.get_images()
            for _ in images:
                lines.append("【图片】")

            lines.append("")
            check_memory()
    finally:
        doc.close()
        if pdf_plumber is not None:
            try:
                pdf_plumber.close()
            except Exception:
                pass

    result = "\n".join(lines).strip()
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(result)


# ============================================================
# PDF → DOCX（pdf2docx）
# ============================================================

def pdf_to_docx(input_path: str, output_path: str):
    try:
        from pdf2docx import Converter
    except ImportError:
        raise RuntimeError(
            "pdf2docx 模块未安装，请执行: pip install pdf2docx"
        )

    cv = Converter(input_path)
    cv.convert(output_path)
    cv.close()

    # pdf2docx 对无文字层 PDF 可能静默产出空文件，显式报错而非假成功
    if not os.path.isfile(output_path) or os.path.getsize(output_path) == 0:
        raise RuntimeError(
            "转换结果为空：源 PDF 可能没有可提取的文本内容（如扫描版/纯图片 PDF）"
        )


# ============================================================
# 降级操作（超大文件仅提取纯文本）
# ============================================================

def pdf_to_html_light(input_path: str, output_path: str):
    """超大文件降级：PDF → HTML（仅纯文本，无样式/表格/图片）"""
    import fitz
    from html import escape

    parts = []
    doc = fitz.open(input_path)
    try:
        for page in doc:
            text = page.get_text("text")
            if text:
                for line in text.strip().split("\n"):
                    line = line.strip()
                    if line:
                        parts.append(f"<p>{escape(line)}</p>")
            check_memory()
    finally:
        doc.close()

    title = os.path.splitext(os.path.basename(input_path))[0]
    html = (
        "<!DOCTYPE html>\n<html>\n<head>\n"
        f'<meta charset="utf-8">\n<title>{escape(title)}</title>\n'
        "</head>\n<body>\n"
        + "\n".join(parts)
        + "\n</body>\n</html>"
    )
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(html)


def pdf_to_md_light(input_path: str, output_path: str):
    """超大文件降级：PDF → Markdown（仅纯文本）"""
    import fitz

    lines = []
    doc = fitz.open(input_path)
    try:
        for page in doc:
            text = page.get_text("text")
            if text:
                lines.append(text.strip())
            check_memory()
    finally:
        doc.close()

    result = "\n\n".join(lines).strip()
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(result)


# ============================================================
# 其他格式 → PDF
# ============================================================

def docx_to_pdf(input_path: str, output_path: str):
    """DOCX → PDF：python-docx 读取 + reportlab 生成（支持图片 + 文档顺序）"""
    from docx import Document
    from docx.oxml.ns import qn
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
    from reportlab.lib.units import mm
    from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, Image as RLImage
    from reportlab.lib import colors
    from reportlab.lib.enums import TA_LEFT, TA_CENTER
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.pdfbase.cidfonts import UnicodeCIDFont
    import io

    # 注册 CJK 字体
    try:
        pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
        cjk_font = "STSong-Light"
    except Exception:
        try:
            pdfmetrics.registerFont(UnicodeCIDFont("HeiseiMin-W3"))
            cjk_font = "HeiseiMin-W3"
        except Exception:
            cjk_font = "Helvetica"

    doc = Document(input_path)
    pdf_doc = SimpleDocTemplate(
        output_path, pagesize=A4,
        leftMargin=20*mm, rightMargin=20*mm,
        topMargin=20*mm, bottomMargin=20*mm,
    )

    styles = getSampleStyleSheet()
    normal_style = ParagraphStyle(
        "CJKNormal", parent=styles["Normal"],
        fontName=cjk_font, fontSize=11, leading=18, alignment=TA_LEFT,
    )
    heading_styles = {}
    for i in range(1, 7):
        heading_styles[i] = ParagraphStyle(
            f"CJKHeading{i}", parent=styles[f"Heading{i}"],
            fontName=cjk_font, fontSize=max(22 - i * 2, 11),
            leading=28, spaceBefore=12, spaceAfter=6, alignment=TA_LEFT,
        )
    # 图片占位符样式
    img_placeholder_style = ParagraphStyle(
        "ImgPlaceholder", parent=normal_style,
        textColor=colors.grey, fontSize=9, alignment=TA_CENTER,
    )

    # 建立 rId → 图片 part 映射
    image_map = {}
    for rel in doc.part.rels.values():
        if "image" in rel.reltype:
            image_map[rel.rId] = rel.target_part

    def get_para_images(para):
        """提取段落中的图片: [(rId, blob, content_type), ...]"""
        images = []
        for drawing in para._element.findall('.//' + qn('w:drawing')):
            for blip in drawing.findall('.//' + qn('a:blip')):
                rId = blip.get(qn('r:embed'))
                if rId and rId in image_map:
                    part = image_map[rId]
                    images.append((rId, part.blob, part.content_type))
        return images

    def build_table(tbl_element):
        """构建 reportlab Table 对象"""
        table_data = []
        for tr in tbl_element.findall(qn('w:tr')):
            row = []
            for tc in tr.findall(qn('w:tc')):
                cell_text = ""
                for p in tc.findall(qn('w:p')):
                    t = "".join(node.text or "" for node in p.findall('.//' + qn('w:t')))
                    if cell_text:
                        cell_text += "\n"
                    cell_text += t
                row.append(_xml_escape(cell_text))
            if row:
                table_data.append(row)
        if not table_data:
            return None
        t = Table(table_data)
        t.setStyle(TableStyle([
            ('GRID', (0, 0), (-1, -1), 0.5, colors.grey),
            ('BACKGROUND', (0, 0), (-1, 0), colors.lightgrey),
            ('FONTNAME', (0, 0), (-1, -1), cjk_font),
            ('FONTSIZE', (0, 0), (-1, -1), 10),
            ('ALIGN', (0, 0), (-1, -1), 'LEFT'),
            ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'),
        ]))
        return t

    # 按文档顺序遍历 body 子元素（段落、表格交替出现）
    story = []
    page_width = A4[0] - 40 * mm   # 可用宽度
    page_height = A4[1] - 40 * mm  # 可用高度（留安全余量）

    for element in doc.element.body:
        tag = element.tag.split('}')[-1] if '}' in element.tag else element.tag

        if tag == 'p':
            from docx.text.paragraph import Paragraph as DocxParagraph
            para = DocxParagraph(element, doc)
            text = para.text.strip()
            images = get_para_images(para)

            # 添加文本（转义 XML 特殊字符，防 reportlab 解析失败）
            if text:
                style = normal_style
                if para.style and para.style.name and para.style.name.startswith("Heading"):
                    try:
                        level = int(para.style.name.replace("Heading", "").strip())
                        style = heading_styles.get(level, heading_styles[1])
                    except (ValueError, KeyError):
                        style = heading_styles[1]
                story.append(Paragraph(_xml_escape(text), style))
            elif not images:
                story.append(Spacer(1, 6))

            # 添加图片
            for rId, blob, content_type in images:
                try:
                    img = RLImage(io.BytesIO(blob))
                    # 等比缩放，不超过页面宽度和高度
                    max_w = page_width
                    max_h = page_height * 0.8  # 留 20% 余量给前后文字
                    w, h = img.imageWidth, img.imageHeight
                    if w > max_w or h > max_h:
                        ratio = min(max_w / w, max_h / h)
                        img.drawWidth = w * ratio
                        img.drawHeight = h * ratio
                    story.append(img)
                except Exception:
                    story.append(Paragraph("【图片】", img_placeholder_style))

        elif tag == 'tbl':
            tbl_table = build_table(element)
            if tbl_table:
                story.append(tbl_table)

    pdf_doc.build(story)


def md_to_pdf(input_path: str, output_path: str):
    """Markdown → PDF：markdown → HTML → xhtml2pdf（纯 Python，免 GTK）"""
    import markdown as md_lib
    from xhtml2pdf import pisa

    # 注册 CJK 字体（防中文乱码）
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.cidfonts import UnicodeCIDFont
    try:
        pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
    except Exception:
        pass

    md_content = _read_text(input_path)

    html_body = md_lib.markdown(
        md_content,
        extensions=["tables", "fenced_code", "codehilite", "toc"],
    )

    title = os.path.splitext(os.path.basename(input_path))[0]
    full_html = (
        "<!DOCTYPE html>\n<html>\n<head>\n"
        f'<meta charset="utf-8">\n<title>{title}</title>\n'
        "<style>\n"
        "@page { size: A4; margin: 2cm; }\n"
        "body { font-family: 'STSong-Light', 'SimSun', 'Microsoft YaHei', sans-serif; font-size: 12pt; line-height: 1.8; }\n"
        "h1 { font-size: 24pt; font-weight: bold; }\n"
        "h2 { font-size: 18pt; font-weight: bold; }\n"
        "h3 { font-size: 14pt; font-weight: bold; }\n"
        "pre { background: #f5f5f5; padding: 12px; border-radius: 4px; font-size: 10pt; }\n"
        "code { font-family: 'Courier New', monospace; font-size: 10pt; }\n"
        "table { border-collapse: collapse; width: 100%; margin: 10px 0; }\n"
        "th, td { border: 1px solid #999; padding: 6px 10px; text-align: left; }\n"
        "th { background: #D9EAD3; font-weight: bold; }\n"
        "blockquote { border-left: 4px solid #ddd; margin: 10px 0; padding-left: 16px; color: #666; }\n"
        "img { max-width: 100%; }\n"
        "hr { border: none; border-top: 1px solid #ccc; margin: 16px 0; }\n"
        ".highlight { background: #fffde7; padding: 2px 4px; }\n"
        "</style>\n"
        "</head>\n<body>\n"
        + html_body
        + "\n</body>\n</html>"
    )

    with open(output_path, "wb") as f:
        status = pisa.CreatePDF(full_html, dest=f, encoding="utf-8")
    # xhtml2pdf 渲染失败时不抛异常只置 err，不检查就是假成功
    if getattr(status, "err", 0):
        raise RuntimeError("xhtml2pdf 渲染失败：Markdown 内容可能含有无法渲染的元素")


def html_to_pdf(input_path: str, output_path: str):
    """HTML → PDF：xhtml2pdf（纯 Python，免 GTK）"""
    from xhtml2pdf import pisa

    # 注册 CJK 字体
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.cidfonts import UnicodeCIDFont
    try:
        pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
    except Exception:
        pass

    html_content = _read_text(input_path)

    with open(output_path, "wb") as f:
        status = pisa.CreatePDF(html_content, dest=f, encoding="utf-8")
    # xhtml2pdf 渲染失败时不抛异常只置 err，不检查就是假成功
    if getattr(status, "err", 0):
        raise RuntimeError("xhtml2pdf 渲染失败：HTML 内容可能含有无法渲染的元素")


def txt_to_pdf(input_path: str, output_path: str):
    """纯文本 → PDF：reportlab"""
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
    from reportlab.lib.units import mm
    from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer
    from reportlab.lib.enums import TA_LEFT
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.cidfonts import UnicodeCIDFont

    try:
        pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
        cjk_font = "STSong-Light"
    except Exception:
        try:
            pdfmetrics.registerFont(UnicodeCIDFont("HeiseiMin-W3"))
            cjk_font = "HeiseiMin-W3"
        except Exception:
            cjk_font = "Helvetica"

    text = _read_text(input_path)

    pdf_doc = SimpleDocTemplate(
        output_path, pagesize=A4,
        leftMargin=20*mm, rightMargin=20*mm,
        topMargin=20*mm, bottomMargin=20*mm,
    )

    styles = getSampleStyleSheet()
    normal_style = ParagraphStyle(
        "CJKNormal", parent=styles["Normal"],
        fontName=cjk_font, fontSize=11, leading=18, alignment=TA_LEFT,
    )

    story = []
    for line in text.split("\n"):
        if not line.strip():
            story.append(Spacer(1, 6))
        else:
            # 转义 XML 特殊字符，防 reportlab 解析失败
            story.append(Paragraph(_xml_escape(line), normal_style))

    pdf_doc.build(story)


# ============================================================
# 主入口
# ============================================================

OPERATIONS = {
    "pdf_to_txt": pdf_to_txt,
    "pdf_to_html": pdf_to_html,
    "pdf_to_md": pdf_to_md,
    "pdf_to_docx": pdf_to_docx,
    "docx_to_pdf": docx_to_pdf,
    "md_to_pdf": md_to_pdf,
    "html_to_pdf": html_to_pdf,
    "txt_to_pdf": txt_to_pdf,
    "pdf_to_html_light": pdf_to_html_light,
    "pdf_to_md_light": pdf_to_md_light,
}


def main():
    global _MEMORY_LIMIT_MB
    if len(sys.argv) != 4:
        print(
            f"用法: python {sys.argv[0]} <operation> <input_path> <output_path>",
            file=sys.stderr,
        )
        print(f"支持的操作: {', '.join(OPERATIONS.keys())}", file=sys.stderr)
        sys.exit(1)

    operation = sys.argv[1]
    input_path = sys.argv[2]
    output_path = sys.argv[3]

    if operation not in OPERATIONS:
        print(f"ERROR: 未知操作 {operation}", file=sys.stderr)
        print(f"支持的操作: {', '.join(OPERATIONS.keys())}", file=sys.stderr)
        sys.exit(1)

    if not os.path.isfile(input_path):
        print(f"ERROR: 输入文件不存在: {input_path}", file=sys.stderr)
        sys.exit(1)

    # 内存上限自适应：文件体积需求 / 设备物理内存预算 取最小，保底 512MB
    try:
        file_mb = os.path.getsize(input_path) / 1024 / 1024
        _MEMORY_LIMIT_MB = _compute_memory_limit(file_mb)
    except OSError:
        pass

    try:
        OPERATIONS[operation](input_path, output_path)
        if os.path.isfile(output_path):
            size = os.path.getsize(output_path)
            if size == 0:
                # 空输出 = 转换实际失败（如扫描版 PDF 无文字层），必须报错而非假成功
                print(
                    "ERROR: 转换结果为空：源文件可能没有可提取的文本内容"
                    "（如扫描版/纯图片 PDF），暂不支持 OCR 识别",
                    file=sys.stderr,
                )
                sys.exit(1)
            print(f"OK:{size}")
        else:
            print("ERROR: 输出文件未生成", file=sys.stderr)
            sys.exit(1)
    except MemoryError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)
    except TimeoutError:
        print("ERROR: 处理超时，请重试", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
