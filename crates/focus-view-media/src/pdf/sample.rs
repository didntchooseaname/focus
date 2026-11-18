pub(crate) fn file(objects: &[String], trailer_extra: &str) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R {trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn stream(content: &str) -> String {
    format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )
}

pub(crate) fn document() -> Vec<u8> {
    let text = |y: f32, s: &str| format!("BT /F1 12 Tf 72 {y} Td ({s}) Tj ET");
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 10 0 R /Dests << /annex [5 0 R /Fit] >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 6 0 R /Resources << /Font << /F1 9 0 R >> >> /Annots [14 0 R 15 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 7 0 R /Resources << /Font << /F1 9 0 R >> >> >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 842 595] /Contents 8 0 R /Resources << /Font << /F1 9 0 R >> >> >>".to_string(),
        stream(&format!("0 g 72 750 100 20 re f {}", text(700., "Findings summary"))),
        stream(&format!("{} {}", text(780., "Second page"), text(400., "Findings summary"))),
        stream(&text(500., "Annex")),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
        "<< /Type /Outlines /First 11 0 R /Last 13 0 R /Count 3 >>".to_string(),
        "<< /Title (Summary) /Parent 10 0 R /Next 13 0 R /First 12 0 R /Last 12 0 R /Count 1 /Dest [3 0 R /XYZ 0 842 0] >>".to_string(),
        "<< /Title <FEFF004400E9007400610069006C0073> /Parent 11 0 R /A << /S /GoTo /D [4 0 R /XYZ 0 442 0] >> >>".to_string(),
        "<< /Title (Annex) /Parent 10 0 R /Prev 11 0 R /Dest /annex >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [72 696 200 712] /Border [0 0 0] /A << /S /GoTo /D [5 0 R /Fit] >> >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [72 600 200 620] /A << /S /URI /URI (https://example.org/report) >> >>".to_string(),
    ];
    file(&objects, "")
}

pub(crate) fn protected() -> Vec<u8> {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>".to_string(),
        format!(
            "<< /Filter /Standard /V 2 /R 3 /Length 128 /P -4 /O <{}> /U <{}> >>",
            "11".repeat(32),
            "22".repeat(32)
        ),
    ];
    file(
        &objects,
        &format!("/Encrypt 4 0 R /ID [<{0}> <{0}>]", "ab".repeat(16)),
    )
}
