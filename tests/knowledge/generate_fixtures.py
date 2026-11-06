"""Reproducible synthetic parser inputs; no customer content or external resources."""
from pathlib import Path
from io import BytesIO
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED
import zlib

ROOT = Path(__file__).parent / 'fixtures'
ROOT.mkdir(exist_ok=True)

def pdf(pages, compressed=False):
    objects = [b'', b'<< /Type /Catalog /Pages 2 0 R >>', b'']
    objects.append(b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>')
    kids=[]
    for text in pages:
        page_id=len(objects); stream_id=page_id+1
        kids.append(f'{page_id} 0 R')
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {stream_id} 0 R >>'.encode())
        stream=f'BT /F1 12 Tf 50 700 Td ({text}) Tj ET'.encode() if text else b''
        if compressed: stream=zlib.compress(stream, level=9)
        filter_value=' /Filter /FlateDecode' if compressed else ''
        objects.append(f'<< /Length {len(stream)}{filter_value} >>\nstream\n'.encode()+stream+b'\nendstream')
    objects[2]=f'<< /Type /Pages /Kids [{" ".join(kids)}] /Count {len(pages)} >>'.encode()
    output=bytearray(b'%PDF-1.4\n'); offsets=[0]
    for i,obj in enumerate(objects[1:],1):
        offsets.append(len(output));output.extend(f'{i} 0 obj\n'.encode()+obj+b'\nendobj\n')
    xref=len(output);output.extend(f'xref\n0 {len(objects)}\n0000000000 65535 f \n'.encode())
    for off in offsets[1:]:output.extend(f'{off:010d} 00000 n \n'.encode())
    output.extend(f'trailer\n<< /Size {len(objects)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
    return bytes(output)

def docx(document, styles=None):
    out=BytesIO()
    with ZipFile(out,'w') as z:
        for name,data in [('word/document.xml',document),('word/styles.xml',styles)]:
            if data is None:continue
            info=ZipInfo(name,(2024,1,1,0,0,0));info.compress_type=ZIP_DEFLATED
            z.writestr(info,data.encode())
    return out.getvalue()

NS='http://schemas.openxmlformats.org/wordprocessingml/2006/main'
(ROOT/'unicode.txt').write_bytes('泵  24V / 1.25A\r\n\r\nEnglish\tvalue: 3,200.5\r\n'.encode())
(ROOT/'manual.md').write_text('# 电气参数\n\n泵的额定电压为24V。\n\n## Safety\n\nKeep dry.\n',encoding='utf-8')
(ROOT/'empty.txt').write_bytes(b'  \r\n')
(ROOT/'invalid.txt').write_bytes(b'\xff\xfe\x00')
(ROOT/'two-pages.pdf').write_bytes(pdf(['PAGE ONE: pump voltage 24V','PAGE TWO: valve pressure 5.5 bar']))
(ROOT/'scanned.pdf').write_bytes(pdf(['']))
(ROOT/'corrupt.docx').write_bytes(b'PK\x03\x04broken')
(ROOT/'structured.docx').write_bytes(docx(f'''<?xml version="1.0" encoding="UTF-8"?><x:document xmlns:x="{NS}"><x:body><x:p><x:pPr><x:pStyle x:val="HeadingCustom"/></x:pPr><x:r><x:t>Electrical</x:t></x:r></x:p><x:p><x:r><x:t>Pump &amp; valve</x:t><x:tab/><x:t>24V</x:t><x:br/><x:t>Rated &lt; 40A</x:t></x:r></x:p><x:tbl><x:tr><x:tc><x:p><x:r><x:t>Cell A</x:t></x:r></x:p></x:tc><x:tc><x:p><x:r><x:t>Cell B</x:t></x:r></x:p></x:tc></x:tr></x:tbl></x:body></x:document>''',f'''<w:styles xmlns:w="{NS}"><w:style w:type="paragraph" w:styleId="HeadingCustom"><w:name w:val="Heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style></w:styles>'''))

# Small on disk, deliberately larger than the integration worker's 4 MiB heap.
(ROOT/'inflated.pdf').write_bytes(pdf(['x' * (8 * 1024 * 1024)], compressed=True))
(ROOT/'inflated.docx').write_bytes(docx(f'<w:document xmlns:w="{NS}"><w:body><!--' + 'x' * (8 * 1024 * 1024) + '--><w:p><w:r><w:t>24V</w:t></w:r></w:p></w:body></w:document>'))
