/**
 * DOCX 转换器
 *
 * DOCX→TXT: quick-xml 流式解析 word/document.xml，提取 <w:t>
 * DOCX→HTML: 流式解析→映射 HTML 标签
 * DOCX→MD: 流式解析→映射 MD 语法
 */

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader as XmlReader;

use super::{html_escape, inline_format};

// ============================================================
// DOCX 解析核心
// ============================================================

/// DOCX 文档元素（段落或表格）
#[derive(Debug, Clone)]
enum DocxElement {
    Paragraph {
        runs: Vec<DocxRun>,
        style: Option<String>,
        /// r:embed 值（如 "rId5"），用于精确映射到 word/media/ 中的图片文件名
        image_rid: Option<String>,
        num_level: Option<u32>,
    },
    Table {
        rows: Vec<Vec<Vec<DocxRun>>>,
    },
}

/// DOCX 文本运行（最小格式单元）
#[derive(Debug, Clone)]
struct DocxRun {
    text: String,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    highlight: Option<String>,
}

/// 去除 XML 标签名的命名空间前缀
fn strip_ns(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    match s.find(':') {
        Some(i) => s[i + 1..].to_string(),
        None => s.to_string(),
    }
}

/// 从 docx zip 中读取 document.xml（自动处理编码）
fn read_docx_xml(data: &[u8]) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("无法打开 docx 文件: {}", e))?;
    let mut file = archive
        .by_name("word/document.xml")
        .map_err(|e| format!("docx 中找不到 document.xml: {}", e))?;
    let mut xml_bytes = Vec::new();
    file.read_to_end(&mut xml_bytes)
        .map_err(|e| format!("读取 document.xml 失败: {}", e))?;

    // 检测并处理编码
    if xml_bytes.len() >= 2 {
        if xml_bytes[0] == 0xFF && xml_bytes[1] == 0xFE {
            let (decoded, _, _) = encoding_rs::UTF_16LE.decode(&xml_bytes[2..]);
            return Ok(decoded.into_owned().into_bytes());
        }
        if xml_bytes[0] == 0xFE && xml_bytes[1] == 0xFF {
            let (decoded, _, _) = encoding_rs::UTF_16BE.decode(&xml_bytes[2..]);
            return Ok(decoded.into_owned().into_bytes());
        }

        let search_len = std::cmp::min(xml_bytes.len(), 512);
        let head = &xml_bytes[..search_len];
        let (head_str, _, _) = encoding_rs::UTF_16LE.decode(head);
        let head_lower = head_str.to_lowercase();
        if head_lower.contains("encoding=\"utf-16\"")
            || head_lower.contains("encoding='utf-16'")
            || head_lower.contains("encoding=\"utf-16le\"")
        {
            let (decoded, _, _) = encoding_rs::UTF_16LE.decode(&xml_bytes);
            return Ok(decoded.into_owned().into_bytes());
        }

        if xml_bytes.len() >= 64 {
            let sample_len = std::cmp::min(xml_bytes.len(), 256);
            let zero_count = xml_bytes[..sample_len]
                .iter()
                .enumerate()
                .filter(|(i, &b)| i % 2 == 1 && b == 0)
                .count();
            if zero_count > sample_len / 5 {
                let (decoded, _, _) = encoding_rs::UTF_16LE.decode(&xml_bytes);
                let trimmed = decoded.trim_start();
                if trimmed.starts_with('<') || trimmed.starts_with("<?") {
                    return Ok(decoded.into_owned().into_bytes());
                }
            }
        }
    }

    Ok(xml_bytes)
}

/// 从 docx 的 XML 中解析文档元素
fn parse_docx_elements(xml_bytes: &[u8]) -> Vec<DocxElement> {
    let mut reader = XmlReader::from_reader(xml_bytes);
    let mut buf = Vec::new();
    let mut elements: Vec<DocxElement> = Vec::new();

    let mut in_body = false;
    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_r_pr = false;
    let mut in_p_pr = false;
    let mut in_t = false;

    let mut p_runs: Vec<DocxRun> = Vec::new();
    let mut p_style: Option<String> = None;
    let mut p_image_rid: Option<String> = None;
    let mut p_num_level: Option<u32> = None;

    let mut run_text = String::new();
    let mut run_bold = false;
    let mut run_italic = false;
    let mut run_underline = false;
    let mut run_strikethrough = false;
    let mut run_highlight: Option<String> = None;

    let mut in_table = false;
    let mut table_rows: Vec<Vec<Vec<DocxRun>>> = Vec::new();
    let mut current_row: Vec<Vec<DocxRun>> = Vec::new();
    let mut current_cell_runs: Vec<DocxRun> = Vec::new();
    let mut in_table_para = false;

    // drawing 跟踪（用于捕获图片 r:embed）
    let mut in_drawing = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(ref e)) | Ok(XmlEvent::Empty(ref e)) => {
                let tag = strip_ns(e.name().as_ref());
                match tag.as_str() {
                    "body" => in_body = true,
                    "tbl" if in_body => {
                        in_table = true;
                        table_rows.clear();
                        current_row.clear();
                    }
                    "tr" if in_table => {
                        current_row.clear();
                        current_cell_runs.clear();
                    }
                    "tc" if in_table => {
                        current_cell_runs.clear();
                    }
                    "p" if in_body || in_table_para => {
                        if in_table && !in_table_para {
                            in_table_para = true;
                        }
                        in_paragraph = true;
                        p_runs.clear();
                        p_style = None;
                        p_image_rid = None;
                        p_num_level = None;
                    }
                    "r" if in_paragraph => {
                        in_run = true;
                        run_text.clear();
                        run_bold = false;
                        run_italic = false;
                        run_underline = false;
                        run_strikethrough = false;
                        run_highlight = None;
                    }
                    "pPr" if in_paragraph => in_p_pr = true,
                    "rPr" if in_run => in_r_pr = true,
                    "t" if in_run => in_t = true,
                    "b" if in_r_pr => run_bold = true,
                    "i" if in_r_pr => run_italic = true,
                    "u" if in_r_pr => run_underline = true,
                    "strike" if in_r_pr => run_strikethrough = true,
                    "highlight" if in_r_pr => {
                        if let Some(val) = e.attributes().flatten().find(|a| {
                            strip_ns(a.key.as_ref()) == "val"
                        }) {
                            run_highlight = Some(
                                String::from_utf8_lossy(&val.value).to_string()
                            );
                        }
                    }
                    "numId" if in_p_pr => {
                        if let Some(val) = e.attributes().flatten().find(|a| {
                            strip_ns(a.key.as_ref()) == "val"
                        }) {
                            let s = String::from_utf8_lossy(&val.value);
                            if let Ok(v) = s.parse::<u32>() {
                                if v > 0 {
                                    p_num_level = Some(1);
                                }
                            }
                        }
                    }
                    "drawing" | "pict" if in_run => {
                        p_image_rid = Some(String::new()); // 标记有图片（rid 稍后从 blip 获取）
                        in_drawing = true;
                    }
                    // 捕获 <a:blip r:embed="rId5"/> 中的 r:embed 值
                    "blip" if in_drawing => {
                        if let Some(val) = e.attributes().flatten().find(|a| {
                            strip_ns(a.key.as_ref()) == "embed"
                        }) {
                            p_image_rid = Some(String::from_utf8_lossy(&val.value).to_string());
                        }
                    }
                    "pStyle" if in_p_pr && !in_table => {
                        if let Some(val) = e.attributes().flatten().find(|a| {
                            strip_ns(a.key.as_ref()) == "val"
                        }) {
                            p_style = Some(
                                String::from_utf8_lossy(&val.value).to_string()
                            );
                        }
                    }
                    _ => {}
                }
            }
            Ok(XmlEvent::Text(ref e)) if in_t => {
                match e.unescape() {
                    Ok(cow) => run_text.push_str(&cow),
                    Err(_) => run_text.push_str(&String::from_utf8_lossy(e.as_ref())),
                }
            }
            Ok(XmlEvent::End(ref e)) => {
                let tag = strip_ns(e.name().as_ref());
                match tag.as_str() {
                    "body" => in_body = false,
                    "p" if in_paragraph => {
                        let para = DocxElement::Paragraph {
                            runs: p_runs.clone(),
                            style: p_style.clone(),
                            image_rid: p_image_rid.clone(),
                            num_level: p_num_level,
                        };
                        if in_table && in_table_para {
                            current_cell_runs.extend(p_runs.clone());
                            in_table_para = false;
                        } else if !p_runs.is_empty() || p_image_rid.is_some() {
                            elements.push(para);
                        }
                        in_paragraph = false;
                    }
                    "r" if in_run => {
                        let r = DocxRun {
                            text: run_text.clone(),
                            bold: run_bold,
                            italic: run_italic,
                            underline: run_underline,
                            strikethrough: run_strikethrough,
                            highlight: run_highlight.clone(),
                        };
                        if in_table && in_table_para {
                            current_cell_runs.push(r);
                        } else if in_paragraph {
                            p_runs.push(r);
                        }
                        in_run = false;
                    }
                    "tc" if in_table => {
                        current_row.push(std::mem::take(&mut current_cell_runs));
                    }
                    "tr" if in_table => {
                        table_rows.push(std::mem::take(&mut current_row));
                    }
                    "tbl" if in_table => {
                        elements.push(DocxElement::Table {
                            rows: std::mem::take(&mut table_rows),
                        });
                        in_table = false;
                    }
                    "pPr" => in_p_pr = false,
                    "rPr" => in_r_pr = false,
                    "t" => in_t = false,
                    _ => {}
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(e) => {
                eprintln!("DOCX XML 解析警告: {}", e);
            }
            _ => {}
        }
        buf.clear();
    }
    elements
}

// ============================================================
// 输出转换
// ============================================================

/// DOCX 元素 → 纯文本
fn docx_elements_to_text(elements: &[DocxElement]) -> String {
    let mut lines = Vec::new();
    for elem in elements {
        match elem {
            DocxElement::Paragraph { runs, image_rid, .. } => {
                let text: String = runs.iter().map(|r| {
                    let mut t = r.text.as_str().to_string();
                    if r.strikethrough { t = format!("~~{}~~", t); }
                    if r.highlight.is_some() { t = format!("«{}»", t); }
                    t
                }).collect();
                let mut line = text;
                if image_rid.is_some() { line.push_str("【图片】"); }
                lines.push(line);
            }
            DocxElement::Table { rows } => {
                for row in rows {
                    let cells: Vec<String> = row.iter().map(|cell| {
                        cell.iter().map(|r| {
                            let mut t = r.text.as_str().to_string();
                            if r.strikethrough { t = format!("~~{}~~", t); }
                            if r.highlight.is_some() { t = format!("«{}»", t); }
                            t
                        }).collect::<String>()
                    }).collect();
                    lines.push(cells.join("\t"));
                }
            }
        }
    }
    lines.join("\n")
}

/// 根据 Word 样式名映射到 HTML 标签
fn docx_style_to_tag(style: &Option<String>) -> &str {
    match style.as_deref() {
        Some(s) => match s.to_lowercase().as_str() {
            "heading1" | "1" => "h1",
            "heading2" | "2" => "h2",
            "heading3" | "3" => "h3",
            "heading4" | "4" => "h4",
            "heading5" | "5" => "h5",
            "heading6" | "6" => "h6",
            "listparagraph" => "li",
            _ => "p",
        },
        None => "p",
    }
}

/// Run 级 HTML 内联格式
fn run_to_html(text: &str, bold: bool, italic: bool, underline: bool, strikethrough: bool, highlight: &Option<String>) -> String {
    let mut result = text.to_string();
    if italic { result = format!("<em>{}</em>", result); }
    if bold { result = format!("<strong>{}</strong>", result); }
    if underline { result = format!("<u>{}</u>", result); }
    if strikethrough { result = format!("<del>{}</del>", result); }
    if highlight.is_some() { result = format!("<mark>{}</mark>", result); }
    result
}

/// Run 级 Markdown 内联格式
fn run_to_md_inline(text: &str, bold: bool, italic: bool, underline: bool, strikethrough: bool, highlight: &Option<String>) -> String {
    let mut result = text.to_string();
    if italic { result = format!("*{}*", result); }
    if bold { result = format!("**{}**", result); }
    if underline { result = format!("<u>{}</u>", result); }
    if strikethrough { result = format!("~~{}~~", result); }
    if highlight.is_some() { result = format!("«{}»", result); }
    result
}

/// DOCX 元素 → Markdown
fn docx_elements_to_md(elements: &[DocxElement]) -> String {
    let mut md = String::new();
    for elem in elements {
        match elem {
            DocxElement::Paragraph { runs, style, image_rid, num_level, .. } => {
                let all_empty = runs.iter().all(|r| r.text.trim().is_empty());
                if all_empty && image_rid.is_none() {
                    md.push('\n');
                    continue;
                }
                let prefix = match style.as_deref() {
                    Some(s) => match s.to_lowercase().as_str() {
                        "heading1" | "1" => "# ",
                        "heading2" | "2" => "## ",
                        "heading3" | "3" => "### ",
                        "heading4" | "4" => "#### ",
                        "heading5" | "5" => "##### ",
                        "heading6" | "6" => "###### ",
                        _ => "",
                    },
                    None => "",
                };
                let list_prefix = if num_level.is_some() && prefix.is_empty() { "- " } else { "" };
                md.push_str(prefix);
                md.push_str(list_prefix);
                for run in runs {
                    let text = inline_format(&run.text);
                    md.push_str(&run_to_md_inline(&text, run.bold, run.italic, run.underline, run.strikethrough, &run.highlight));
                }
                if image_rid.is_some() { md.push_str("【图片】"); }
                md.push_str("\n\n");
            }
            DocxElement::Table { rows } => {
                if rows.is_empty() { continue; }
                let col_count = rows[0].len();
                for (ri, row) in rows.iter().enumerate() {
                    md.push('|');
                    for ci in 0..col_count {
                        let cell_text = if ci < row.len() {
                            row[ci].iter().map(|r| {
                                run_to_md_inline(&r.text, r.bold, r.italic, r.underline, r.strikethrough, &r.highlight)
                            }).collect::<String>()
                        } else { String::new() };
                        md.push(' ');
                        md.push_str(&cell_text);
                        md.push_str(" |");
                    }
                    md.push('\n');
                    if ri == 0 {
                        md.push('|');
                        for _ in 0..col_count {
                            md.push_str(" --- |");
                        }
                        md.push('\n');
                    }
                }
                md.push('\n');
            }
        }
    }
    md.trim().to_string()
}

// ============================================================
// 公共转换接口
// ============================================================

/// DOCX → 纯文本
pub fn docx_to_txt(src: &Path, dst: &Path) -> Result<u64, String> {
    let data = std::fs::read(src).map_err(|e| format!("无法读取源文件: {}", e))?;
    let text = extract_docx_text(&data)?;
    std::fs::write(dst, text.trim()).map_err(|e| format!("写入失败: {}", e))?;
    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// 从 DOCX 二进制数据提取纯文本（供 AI 对话附件解析使用）
pub fn extract_docx_text(data: &[u8]) -> Result<String, String> {
    let xml_bytes = read_docx_xml(data)?;
    let elements = parse_docx_elements(&xml_bytes);
    Ok(docx_elements_to_text(&elements))
}

/// 从 DOCX 二进制数据提取 Markdown（保留表格、标题、列表等富文本结构）
pub fn extract_docx_md(data: &[u8]) -> Result<String, String> {
    let xml_bytes = read_docx_xml(data)?;
    let elements = parse_docx_elements(&xml_bytes);
    Ok(docx_elements_to_md(&elements))
}

// ============================================================
// 完整文档提取（HTML + 内嵌图片）— 供 AI 多模态对话使用
// ============================================================

/// 提取出的单张图片
#[derive(Debug, Clone)]
pub struct DocImage {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

/// 根据文件扩展名推断 MIME 类型
fn mime_from_ext(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.ends_with(".png") { return "image/png"; }
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { return "image/jpeg"; }
    if lower.ends_with(".gif") { return "image/gif"; }
    if lower.ends_with(".bmp") { return "image/bmp"; }
    if lower.ends_with(".webp") { return "image/webp"; }
    if lower.ends_with(".svg") { return "image/svg+xml"; }
    if lower.ends_with(".tiff") || lower.ends_with(".tif") { return "image/tiff"; }
    "image/png" // 默认
}

/// 从 DOCX ZIP 包中提取所有内嵌图片
fn extract_images_from_docx(data: &[u8]) -> Vec<DocImage> {
    let cursor = Cursor::new(data);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };

    let mut images: Vec<DocImage> = Vec::new();
    // 收集 word/media/ 下的所有文件名
    let mut media_names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            let name = f.name().to_string();
            if name.starts_with("word/media/") {
                media_names.push(name);
            }
        }
    }

    // 重新打开 archive 读取图片数据
    let cursor2 = Cursor::new(data);
    if let Ok(mut archive2) = zip::ZipArchive::new(cursor2) {
        for name in &media_names {
            if let Ok(mut file) = archive2.by_name(name) {
                let mut buf = Vec::new();
                if file.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                    let simple_name = name.strip_prefix("word/media/").unwrap_or(name);
                    images.push(DocImage {
                        name: simple_name.to_string(),
                        mime: mime_from_ext(simple_name).to_string(),
                        data: buf,
                    });
                }
            }
        }
    }

    images
}

/// 解析 word/_rels/document.xml.rels，建立 rId → 图片文件名的映射
///
/// DOCX 中用 rId（如 "rId5"）引用内嵌资源，rels 文件记录映射关系：
/// ```xml
/// <Relationship Id="rId5" Target="media/image1.png" .../>
/// ```
fn parse_docx_rels(data: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let cursor = Cursor::new(data);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(_) => return map,
    };
    let mut file = match archive.by_name("word/_rels/document.xml.rels") {
        Ok(f) => f,
        Err(_) => return map,
    };
    let mut xml_bytes = Vec::new();
    if file.read_to_end(&mut xml_bytes).is_err() {
        return map;
    }

    let mut reader = XmlReader::from_reader(xml_bytes.as_slice());
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Empty(ref e)) | Ok(XmlEvent::Start(ref e)) => {
                if strip_ns(e.name().as_ref()) == "Relationship" {
                    let mut id = None;
                    let mut target = None;
                    for attr in e.attributes().flatten() {
                        match strip_ns(attr.key.as_ref()).as_str() {
                            "Id" => id = Some(String::from_utf8_lossy(&attr.value).to_string()),
                            "Target" => target = Some(String::from_utf8_lossy(&attr.value).to_string()),
                            _ => {}
                        }
                    }
                    if let (Some(rid), Some(tgt)) = (id, target) {
                        // Target 类似 "media/image1.png"，提取纯文件名
                        let name = tgt.rsplit('/').next().unwrap_or(&tgt).to_string();
                        map.insert(rid, name);
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    map
}

/// DOCX 元素 → HTML（含图片 data URI 内嵌）
fn docx_elements_to_html_full(
    elements: &[DocxElement],
    images: &[DocImage],
    rid_map: &HashMap<String, String>,
) -> String {
    let mut html = String::new();
    let mut img_idx = 0usize;

    for elem in elements {
        match elem {
            DocxElement::Paragraph { runs, style, image_rid, num_level, .. } => {
                let all_empty = runs.iter().all(|r| r.text.trim().is_empty());
                if all_empty && image_rid.is_none() { continue; }

                let tag = docx_style_to_tag(style);

                // 列表项特殊处理
                if num_level.is_some() && tag == "li" {
                    html.push_str("<li>");
                } else if num_level.is_some() {
                    html.push_str(&format!("<li class=\"list-item\">"));
                } else {
                    html.push_str(&format!("<{}>", tag));
                }

                for run in runs {
                    if run.text.is_empty() { continue; }
                    let escaped = html_escape(&run.text);
                    html.push_str(&run_to_html(&escaped, run.bold, run.italic, run.underline, run.strikethrough, &run.highlight));
                }

                if image_rid.is_some() {
                    // 优先用 rId 通过关系文件精确查找图片名
                    let rid = image_rid.as_deref().unwrap_or("");
                    let img_name = rid_map.get(rid).map(|s| s.as_str()).unwrap_or_else(|| {
                        // 回退到顺序索引
                        if img_idx < images.len() { let name = images[img_idx].name.as_str(); img_idx += 1; name } else { "未知图片" }
                    });
                    // 如果精确匹配到了，也要递增 img_idx（因为图片在 images 列表中存在）
                    if rid_map.contains_key(rid) && img_idx < images.len() {
                        img_idx += 1;
                    }
                    html.push_str(&format!("[图片: {}]", html_escape(img_name)));
                }

                if num_level.is_some() {
                    html.push_str("</li>\n");
                } else {
                    html.push_str(&format!("</{}>\n", tag));
                }
            }
            DocxElement::Table { rows } => {
                html.push_str("<table border=\"1\" style=\"border-collapse:collapse\">\n");
                for (ri, row) in rows.iter().enumerate() {
                    html.push_str("<tr>");
                    let cell_tag = if ri == 0 { "th" } else { "td" };
                    for cell in row {
                        html.push_str(&format!("<{}>", cell_tag));
                        for run in cell {
                            let escaped = html_escape(&run.text);
                            html.push_str(&run_to_html(&escaped, run.bold, run.italic, run.underline, run.strikethrough, &run.highlight));
                        }
                        html.push_str(&format!("</{}>", cell_tag));
                    }
                    html.push_str("</tr>\n");
                }
                html.push_str("</table>\n");
            }
        }
    }
    html
}

/// 完整提取 DOCX：HTML 文本 + 内嵌图片列表
///
/// 返回 (html_body, images)。HTML 中图片用 [图片: name] 占位。
/// 前端可用此数据构造多模态 AI 消息（text + image_url）。
pub fn extract_docx_full(data: &[u8]) -> Result<(String, Vec<DocImage>), String> {
    let xml_bytes = read_docx_xml(data)?;
    let elements = parse_docx_elements(&xml_bytes);
    let images = extract_images_from_docx(data);
    let rid_map = parse_docx_rels(data);
    let html = docx_elements_to_html_full(&elements, &images, &rid_map);
    Ok((html, images))
}

/// DOCX → HTML
pub fn docx_to_html(src: &Path, dst: &Path) -> Result<u64, String> {
    let data = std::fs::read(src).map_err(|e| format!("无法读取源文件: {}", e))?;
    let xml_bytes = read_docx_xml(&data)?;
    let elements = parse_docx_elements(&xml_bytes);

    let title = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Converted");

    let mut html = String::from(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n",
    );
    html.push_str(&format!("<title>{}</title>\n</head>\n<body>\n", html_escape(title)));

    for elem in &elements {
        match elem {
            DocxElement::Paragraph { runs, style, image_rid, .. } => {
                let all_empty = runs.iter().all(|r| r.text.trim().is_empty());
                if all_empty && image_rid.is_none() { continue; }
                let tag = docx_style_to_tag(style);
                html.push_str(&format!("<{}>", tag));
                for run in runs {
                    if run.text.is_empty() { continue; }
                    let escaped = html_escape(&run.text);
                    html.push_str(&run_to_html(&escaped, run.bold, run.italic, run.underline, run.strikethrough, &run.highlight));
                }
                if image_rid.is_some() { html.push_str("【图片】"); }
                html.push_str(&format!("</{}>\n", tag));
            }
            DocxElement::Table { rows } => {
                html.push_str("<table border=\"1\">\n");
                for (ri, row) in rows.iter().enumerate() {
                    html.push_str("<tr>");
                    let cell_tag = if ri == 0 { "th" } else { "td" };
                    for cell in row {
                        html.push_str(&format!("<{}>", cell_tag));
                        for run in cell {
                            let escaped = html_escape(&run.text);
                            html.push_str(&run_to_html(&escaped, run.bold, run.italic, run.underline, run.strikethrough, &run.highlight));
                        }
                        html.push_str(&format!("</{}>", cell_tag));
                    }
                    html.push_str("</tr>\n");
                }
                html.push_str("</table>\n");
            }
        }
    }

    html.push_str("</body>\n</html>");
    std::fs::write(dst, &html).map_err(|e| format!("写入失败: {}", e))?;
    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// DOCX → Markdown
pub fn docx_to_md(src: &Path, dst: &Path) -> Result<u64, String> {
    let data = std::fs::read(src).map_err(|e| format!("无法读取源文件: {}", e))?;
    let xml_bytes = read_docx_xml(&data)?;
    let elements = parse_docx_elements(&xml_bytes);
    let md = docx_elements_to_md(&elements);
    std::fs::write(dst, md.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}
