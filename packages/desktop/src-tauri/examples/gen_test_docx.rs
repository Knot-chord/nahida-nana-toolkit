// 临时工具：生成集成测试所需的 DOCX 样本（含中文段落与标题）
use docx_rs::*;

fn main() {
    let doc = Docx::new()
        .add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("纳西妲的 DOCX 测试样本").bold()),
        )
        .add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("第一段落：包含中文与 English。")),
        )
        .add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("第二段落：特殊字符 A & B。")),
        )
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("结尾行：全链路验证。")));

    let mut buf = std::io::Cursor::new(Vec::new());
    doc.build().pack(&mut buf).unwrap();
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../临时/test_files/chain_conversions/test_sample.docx");
    std::fs::write(&out, buf.into_inner()).unwrap();
    println!("已生成: {}", out.display());
}
