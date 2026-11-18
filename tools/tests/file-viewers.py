import os
import struct
import sys
import zlib


def png(path, width, height, colour):
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            band = (x // 80 + y // 80) % 2
            r, g, b = colour if band else (240, 240, 236)
            if abs(x - y * width // height) < 6:
                r, g, b = 20, 20, 20
            rows += bytes((r, g, b))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(bytes(rows), 6)))
        f.write(chunk(b"IEND", b""))


WORDS = (
    "the scope covers the domain controller and the file servers of the head office "
    "each finding gives its evidence its impact and the remediation agreed with the client"
).split()


def page_text(n):
    lines = [f"BT /F2 20 Tf 72 770 Td (Section {n}) Tj ET"]
    y = 730
    for i in range(30):
        words = [WORDS[(n * 7 + i * 3 + k) % len(WORDS)] for k in range(11)]
        if n in (3, 7) and i == 4:
            words[5] = "kerberos"
        lines.append(f"BT /F1 11 Tf 72 {y} Td ({' '.join(words)}) Tj ET")
        y -= 20
    return "\n".join(lines)


def pdf(path, pages=12):
    objects = {}
    catalog, pages_id, font, bold, outlines = 1, 2, 3, 4, 5
    first_page = 10
    page_ids = [first_page + 2 * i for i in range(pages)]
    content_ids = [first_page + 2 * i + 1 for i in range(pages)]
    item_ids = [100, 101, 102, 103]
    link_ids = [110, 111]
    objects[catalog] = f"<< /Type /Catalog /Pages {pages_id} 0 R /Outlines {outlines} 0 R >>"
    kids = " ".join(f"{p} 0 R" for p in page_ids)
    objects[pages_id] = f"<< /Type /Pages /Kids [{kids}] /Count {pages} >>"
    objects[font] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
    objects[bold] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
    for i, (pid, cid) in enumerate(zip(page_ids, content_ids)):
        annots = f" /Annots [{link_ids[0]} 0 R {link_ids[1]} 0 R]" if i == 0 else ""
        objects[pid] = (
            f"<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 595 842] /Contents {cid} 0 R "
            f"/Resources << /Font << /F1 {font} 0 R /F2 {bold} 0 R >> >>{annots} >>"
        )
        content = page_text(i + 1)
        if i == 0:
            content += "\nBT /F1 11 Tf 72 100 Td (See section 5 and https://example.org/scope) Tj ET"
        objects[cid] = f"<< /Length {len(content)} >>\nstream\n{content}\nendstream"
    objects[outlines] = f"<< /Type /Outlines /First {item_ids[0]} 0 R /Last {item_ids[2]} 0 R /Count 4 >>"
    objects[item_ids[0]] = (
        f"<< /Title (Scope) /Parent {outlines} 0 R /Next {item_ids[1]} 0 R "
        f"/Dest [{page_ids[0]} 0 R /XYZ 0 842 0] >>"
    )
    objects[item_ids[1]] = (
        f"<< /Title (Findings) /Parent {outlines} 0 R /Prev {item_ids[0]} 0 R /Next {item_ids[2]} 0 R "
        f"/First {item_ids[3]} 0 R /Last {item_ids[3]} 0 R /Count 1 /Dest [{page_ids[2]} 0 R /XYZ 0 842 0] >>"
    )
    objects[item_ids[3]] = (
        f"<< /Title (Kerberos relay) /Parent {item_ids[1]} 0 R /Dest [{page_ids[6]} 0 R /XYZ 0 842 0] >>"
    )
    objects[item_ids[2]] = (
        f"<< /Title (Remediation) /Parent {outlines} 0 R /Prev {item_ids[1]} 0 R "
        f"/Dest [{page_ids[10]} 0 R /Fit] >>"
    )
    objects[link_ids[0]] = (
        f"<< /Type /Annot /Subtype /Link /Rect [72 96 200 112] /Border [0 0 0] "
        f"/A << /S /GoTo /D [{page_ids[4]} 0 R /Fit] >> >>"
    )
    objects[link_ids[1]] = (
        "<< /Type /Annot /Subtype /Link /Rect [200 96 400 112] /Border [0 0 0] "
        "/A << /S /URI /URI (https://example.org/scope) >> >>"
    )
    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = {}
    for number in sorted(objects):
        offsets[number] = len(out)
        out += f"{number} 0 obj\n{objects[number]}\nendobj\n".encode("latin-1")
    size = max(objects) + 1
    xref = len(out)
    out += f"xref\n0 {size}\n0000000000 65535 f \n".encode()
    for number in range(1, size):
        if number in offsets:
            out += f"{offsets[number]:010} 00000 n \n".encode()
        else:
            out += b"0000000000 65535 f \n"
    out += f"trailer\n<< /Size {size} /Root {catalog} 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    with open(path, "wb") as f:
        f.write(out)


folder = sys.argv[1]
count = int(sys.argv[2]) if len(sys.argv) > 2 else 12
os.makedirs(folder, exist_ok=True)
png(os.path.join(folder, "a-photo.png"), 1600, 1000, (90, 120, 160))
png(os.path.join(folder, "b-photo.png"), 600, 900, (160, 110, 80))
pdf(os.path.join(folder, "report.pdf"), count)
