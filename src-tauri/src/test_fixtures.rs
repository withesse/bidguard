// 测试夹具构造器：手造最小合法 docx / xlsx 文件（cfg(test) 专用，不进发布二进制）。
// 用于端到端测试真实解析器（zip+OOXML 读取、calamine），而非绕过解析直插数据。
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

/// 最小合法 docx：调用方提供 <w:body> 内的 XML（段落/表格皆可）。
pub(crate) fn write_docx_body(dir: &Path, name: &str, body_xml: &str) -> String {
    let p = dir.join(name);
    let f = std::fs::File::create(&p).unwrap();
    let mut zw = zip::ZipWriter::new(f);
    let o = SimpleFileOptions::default();
    zw.start_file("[Content_Types].xml", o).unwrap();
    zw.write_all(r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.as_bytes()).unwrap();
    zw.start_file("word/document.xml", o).unwrap();
    let xml = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body_xml}</w:body></w:document>"#
    );
    zw.write_all(xml.as_bytes()).unwrap();
    zw.finish().unwrap();
    p.to_string_lossy().into_owned()
}

/// 最小合法 xlsx：单工作表，所有单元格为 inline string。
pub(crate) fn write_xlsx_rows(dir: &Path, name: &str, sheet: &str, rows: &[&[&str]]) -> String {
    let p = dir.join(name);
    let f = std::fs::File::create(&p).unwrap();
    let mut zw = zip::ZipWriter::new(f);
    let o = SimpleFileOptions::default();
    zw.start_file("[Content_Types].xml", o).unwrap();
    zw.write_all(r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.as_bytes()).unwrap();
    zw.start_file("_rels/.rels", o).unwrap();
    zw.write_all(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.as_bytes()).unwrap();
    zw.start_file("xl/workbook.xml", o).unwrap();
    zw.write_all(format!(r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{sheet}" sheetId="1" r:id="rId1"/></sheets></workbook>"#).as_bytes()).unwrap();
    zw.start_file("xl/_rels/workbook.xml.rels", o).unwrap();
    zw.write_all(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.as_bytes()).unwrap();
    zw.start_file("xl/worksheets/sheet1.xml", o).unwrap();
    let mut body = String::from(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#);
    for (ri, row) in rows.iter().enumerate() {
        body.push_str(&format!("<row r=\"{}\">", ri + 1));
        for cell in row.iter() {
            body.push_str(&format!("<c t=\"inlineStr\"><is><t>{cell}</t></is></c>"));
        }
        body.push_str("</row>");
    }
    body.push_str("</sheetData></worksheet>");
    zw.write_all(body.as_bytes()).unwrap();
    zw.finish().unwrap();
    p.to_string_lossy().into_owned()
}

/// 含一张报价表的 docx：表头 + 一行明细（价格可变，用于跨格式冲突测试）。
pub(crate) fn write_docx_price_table(dir: &Path, name: &str, price: &str) -> String {
    let cell = |t: &str| format!("<w:tc><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let row = |cells: &[&str]| {
        format!("<w:tr>{}</w:tr>", cells.iter().map(|c| cell(c)).collect::<String>())
    };
    let body = format!(
        "<w:p><w:r><w:t>报价清单如下表所示，所有设备均为原厂正品并提供三年质保服务。</w:t></w:r></w:p><w:tbl>{}{}</w:tbl>",
        row(&["序号", "设备名称及服务内容", "总价", "工期"]),
        row(&["1", "核心交换机及配套光模块安装调试", price, "30天"]),
    );
    write_docx_body(dir, name, &body)
}
