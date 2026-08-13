/**
 * 文本解码核心：编码自适应（全链路唯一入口）
 *
 * 用户文档编码多样，统一识别策略（按优先级）：
 * 1. BOM 探测：UTF-8 BOM / UTF-16 LE BOM / UTF-16 BE BOM → 按对应编码解码并剥离 BOM
 * 2. 无 BOM UTF-16 启发式：偶/奇数位交替出现 0x00（ASCII 范围文本的显著特征）
 * 3. UTF-8 严格解码
 * 4. GB18030 回退（GBK/GB2312 超集，兼容中文 Windows 常见编码）
 * 5. 均失败 → 明确报错，提示另存为 UTF-8
 *
 * 关键：绝不能让 GB18030 兜底"静默吃掉"UTF-16 文件（会解出乱码且不报错），
 * 所以 UTF-16 探测必须排在 GB18030 之前。
 */

use std::path::Path;

/// 从字节解码文本：BOM → UTF-16 启发式 → UTF-8 → GB18030
pub fn decode_text_bytes(bytes: &[u8]) -> Result<String, String> {
    // 1. BOM 探测
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return decode_utf8(&bytes[3..]);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(&bytes[2..], encoding_rs::UTF_16LE);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(&bytes[2..], encoding_rs::UTF_16BE);
    }

    // 2. 无 BOM UTF-16 启发式（ASCII 范围文本：隔字节为 0x00）
    if bytes.len() >= 4 {
        if bytes[1] == 0x00 && bytes[3] == 0x00 {
            return decode_utf16(bytes, encoding_rs::UTF_16LE);
        }
        if bytes[0] == 0x00 && bytes[2] == 0x00 {
            return decode_utf16(bytes, encoding_rs::UTF_16BE);
        }
    }

    // 3. UTF-8 严格解码
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => {
            // 4. GB18030 回退
            let (decoded, _enc, had_errors) = encoding_rs::GB18030.decode(bytes);
            if had_errors {
                Err("无法识别源文件编码：既非 UTF-8/UTF-16 也非 GB18030/GBK，请先将文件另存为 UTF-8 编码".to_string())
            } else {
                Ok(decoded.into_owned())
            }
        }
    }
}

/// 读取文本文件：编码自适应解码
pub fn read_text_flexible(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("无法读取源文件: {}", e))?;
    decode_text_bytes(&bytes)
}

fn decode_utf8(bytes: &[u8]) -> Result<String, String> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => Err("文件带 UTF-8 BOM 但内容不是合法 UTF-8，请检查文件是否损坏".to_string()),
    }
}

fn decode_utf16(bytes: &[u8], enc: &'static encoding_rs::Encoding) -> Result<String, String> {
    let (decoded, _enc, had_errors) = enc.decode(bytes);
    if had_errors {
        Err("UTF-16 解码失败：文件可能已损坏，请检查文件完整性".to_string())
    } else {
        Ok(decoded.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_plain() {
        assert_eq!(decode_text_bytes("你好 world".as_bytes()).unwrap(), "你好 world");
    }

    #[test]
    fn test_utf8_bom_stripped() {
        let mut v = vec![0xEF, 0xBB, 0xBF];
        v.extend_from_slice("# 标题".as_bytes());
        // BOM 必须剥离，否则首行标题语法会被破坏
        assert_eq!(decode_text_bytes(&v).unwrap(), "# 标题");
    }

    #[test]
    fn test_utf16le_with_bom() {
        let mut v = vec![0xFF, 0xFE];
        v.extend("中文测试".encode_utf16().flat_map(|u| u.to_le_bytes()));
        assert_eq!(decode_text_bytes(&v).unwrap(), "中文测试");
    }

    #[test]
    fn test_utf16be_with_bom() {
        let mut v = vec![0xFE, 0xFF];
        v.extend("中文测试".encode_utf16().flat_map(|u| u.to_be_bytes()));
        assert_eq!(decode_text_bytes(&v).unwrap(), "中文测试");
    }

    #[test]
    fn test_utf16le_no_bom_heuristic() {
        // 记事本“Unicode”无 BOM 保存场景：隔字节为 0x00
        let v: Vec<u8> = "hello".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(decode_text_bytes(&v).unwrap(), "hello");
    }

    #[test]
    fn test_gbk_fallback() {
        let (bytes, _, _) = encoding_rs::GBK.encode("你好世界");
        assert_eq!(decode_text_bytes(&bytes).unwrap(), "你好世界");
    }
}
