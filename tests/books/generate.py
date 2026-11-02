#!/usr/bin/env python3
"""Deterministic stdlib-only fixtures. Opera Game PGN and UCI are independent source records.
Regenerate with python3 tests/books/generate.py; no parser output is used as an oracle.
"""
import html
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).parent / 'fixtures'
START = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'
PGN = '''1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5
6. Bc4 Nf6 7. Qb3 Qe7 8. Nc3 c6 9. Bg5 b5 10. Nxb5 cxb5
11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7 14. Rd1 Qe6
15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8# 1-0'''
SAN = [s for s in PGN.split() if not s.endswith('.') and s != '1-0']
UCI = '''e2e4 e7e5 g1f3 d7d6 d2d4 c8g4 d4e5 g4f3 d1f3 d6e5
f1c4 g8f6 f3b3 d8e7 b1c3 c7c6 c1g5 b7b5 c3b5 c6b5 c4b5 b8d7
e1c1 a8d8 d1d7 d8d7 h1d1 e7e6 b5d7 f6d7 b3b8 d7b8 d1d8'''.split()
assert len(SAN) == len(UCI)

def epub(path, pages, images=None):
    entries = {'mimetype': 'application/epub+zip',
      'META-INF/container.xml': '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
      'book.opf': '<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" version="3.0" unique-identifier="book-id"><metadata><dc:identifier id="book-id">urn:woodpeck:opera</dc:identifier><dc:title>Opera Game</dc:title><dc:creator>Paul Morphy</dc:creator><dc:language>en</dc:language><meta property="dcterms:modified">2000-01-01T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>' + ''.join(f'<item id="p{i}" href="p{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(len(pages))) + ''.join(f'<item id="i{i}" href="images/{n}" media-type="image/{"jpeg" if n.endswith(".jpg") else "png"}"/>' for i, n in enumerate(images or {})) + '</manifest><spine>' + ''.join(f'<itemref idref="p{i}"/>' for i in range(len(pages))) + '</spine></package>'}
    entries['nav.xhtml'] = '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="p0.xhtml">Opera Game</a></li></ol></nav></body></html>'
    for i, content in enumerate(pages):
        entries[f'p{i}.xhtml'] = f'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Opera Game</title></head><body>{content}</body></html>'
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as z:
        for name, content in entries.items():
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
            z.writestr(info, content.encode())
        for name, content in (images or {}).items():
            info = zipfile.ZipInfo('images/' + name, (2000, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, content)

def pdf(path, pages, scanned=False):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'', b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
    refs = []
    for lines in pages:
        pageid = len(objects)+1
        refs.append(pageid)
        stream = b'q 400 0 0 500 30 30 cm BI /W 1 /H 1 /BPC 8 /CS /G ID \x80 EI Q' if scanned else b'BT /F1 11 Tf 14 TL 40 750 Td\n' + b'\n'.join(b'(' + s.replace('\\', '\\\\').replace('(', '\\(').replace(')', '\\)').encode('ascii') + b') Tj T*' for s in lines if isinstance(s,str)) + b'\nET'
        if lines and isinstance(lines[0], tuple):
            stream = b'\n'.join(f'BT /F1 11 Tf {x} {y} Td ({text}) Tj ET'.encode() for x,y,text in lines)
        objects.extend([f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {pageid+1} 0 R >>'.encode(), f'<< /Length {len(stream)} >>\nstream\n'.encode()+stream+b'\nendstream'])
    objects[1] = ('<< /Type /Pages /Kids ['+' '.join(f'{n} 0 R' for n in refs)+f'] /Count {len(refs)} >>').encode()
    data = b'%PDF-1.4\n'; offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(data)); data += f'{i} 0 obj\n'.encode()+obj+b'\nendobj\n'
    xref = len(data)
    data += f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'.encode()+b''.join(f'{n:010} 00000 n \n'.encode() for n in offsets[1:])
    data += f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode()
    path.write_bytes(data)

def fragment(anchor, moves, page=0, start=START, **extra):
    return dict(page=page, anchor=anchor, start_fen=start, moves=moves, **extra)

def save(name, pages, fragments, notation, diagrams=(), scanned=False, images=None, sources=None):
    path = ROOT / name
    if path.suffix == '.epub': epub(path, pages, images)
    else: pdf(path, pages, scanned)
    truth = dict(file=name, notation=notation, complete=True, fragments=fragments, diagrams=diagrams, scanned=scanned)
    if sources is not None: truth['diagram_sources'] = sources
    path.with_suffix('.json').write_text(json.dumps(truth, ensure_ascii=False, indent=2)+'\n')

def numbered(moves):
    return ' '.join((f'{i//2+1}. ' if i % 2 == 0 else '') + s for i, s in enumerate(moves))

ROOT.mkdir(exist_ok=True)
text = numbered(SAN)
save('san.epub', ['<h1>Opera Game</h1><p>'+text+'</p>'], [fragment(text, UCI)], 'SAN')
for name, alphabet in [('portuguese', 'RDTBC'), ('spanish', 'RDTAC'), ('german', 'KDTLS'), ('french', 'RDTFC'), ('dutch', 'KDTLP'), ('italian', 'RDTAC'), ('figurine', '♔♕♖♗♘')]:
    trans = str.maketrans('KQRBN', alphabet)
    text = numbered([s.translate(trans) for s in SAN])
    save(name+'.epub', ['<p>'+text+'</p>'], [fragment(text, UCI)], name)
lan = numbered([u[:2]+'-'+u[2:] for u in UCI])
save('lan.epub', ['<p>'+lan+'</p>'], [fragment(lan, UCI)], 'LAN')
# Descriptive rendering uses independently supplied UCI and an initial board, not the parser.
board = {chr(97+f)+str(r+1): piece for r,row in enumerate(['RNBQKBNR','PPPPPPPP','........','........','........','........','pppppppp','rnbqkbnr']) for f,piece in enumerate(row) if piece != '.'}
desc = []
for i, u in enumerate(UCI):
    source, dest = u[:2], u[2:4]
    piece = board.pop(source)
    capture = dest in board
    if SAN[i].startswith('O-O'):
        token = 'Castles' if SAN[i] == 'O-O' else 'O-O-O'
        rook_from, rook_to = ('a1','d1') if i%2 == 0 else ('a8','d8')
        board[rook_to] = board.pop(rook_from)
    else:
        role = 'Kt' if piece.upper() == 'N' else piece.upper()
        targetfile = ['QR','QKt','QB','Q','K','KB','KKt','KR'][ord(dest[0])-97]
        rank = int(dest[1]) if i%2 == 0 else 9-int(dest[1])
        token = role+('x' if capture else '-')+targetfile+str(rank)
        if '+' in SAN[i]: token += ' ch'
        if '#' in SAN[i]: token += ' mate'
    board[dest] = piece
    desc.append('QKt-Q2' if i == 21 else token)
for spaced in (False, True):
    rendered = [s.replace('-', ' - ').replace('Kt','Kt ') if spaced else s for s in desc]
    text = numbered([' '.join(s.split()) for s in rendered])
    save('descriptive'+('-spaces' if spaced else '')+'.epub', ['<p>'+text+'</p>'], [fragment(text,UCI)], 'descriptive')
text = '1. e4! e5 $1 2. Nf3 (2. Bc4 Nf6 [2... Nc6]) d6 3. d4 Bg4'
moves = UCI[:3]+['f1c4','g8f6','b8c6']+UCI[3:6]
parents = [None,0,1,1,3,3,2,6,7]
save('variations.epub', ['<p>'+text+'</p>'], [fragment(text,moves,parents=parents)], 'SAN variations')
text = '1. e4 e5 2. Nf3 Bh9 3. d4'
save('broken.epub', ['<p>'+text+'</p>'], [fragment(text,UCI[:3],unresolved=True)], 'broken')
text = '12. Nf3 Nc6'
save('unknown-fragment.epub', ['<p>'+text+'</p>'], [fragment(text,[],unresolved=True)], 'broken fragment')
text = '1. e4 e5 2. Nf3 d6'
save('table.epub', ['<table><tr><th>White</th><th>Black</th></tr><tr><td>1. e4</td><td>e5</td></tr><tr><td>2. Nf3</td><td>d6</td></tr></table>'], [fragment('White Black '+text,UCI[:4])], 'table')
# PDF page continuation uses the verified position after 1.e4 e5.
mid = 'rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2'
save('pages.pdf', [['1. e4 e5'], ['2. Nf3 d6 3. d4 Bg4']], [fragment('1. e4 e5',UCI[:2]),fragment('2. Nf3 d6 3. d4 Bg4',UCI[2:6],page=1,start=mid)], 'PDF SAN')
save('scanned.pdf', [[]], [], 'scanned', scanned=True)
text = '2. Nf3 d6'
save('fen.epub', ['<p>'+mid+'</p><p>'+text+'</p>'], [fragment(text,UCI[2:4],start=mid)], 'FEN',diagrams=[mid])
grid = '♜♞♝♛♚♝♞♜\n♟♟♟♟♟♟♟♟\n........\n........\n........\n........\n♙♙♙♙♙♙♙♙\n♖♘♗♕♔♗♘♖\nWhite to move'
diagram_fen = START.replace('KQkq','-')
save('diagram.epub', ['<pre>'+grid+'</pre><p>1. e4 e5</p>'], [fragment('1. e4 e5',UCI[:2],start=diagram_fen)], 'Unicode diagram',diagrams=[diagram_fen])
text = '1. e4 e5 2. Nf3 d6'
alt = 'If now 2... Nc6, there follows 3. Bc4'
save('analysis.epub', ['<p>'+text+'</p><p>'+alt+'</p>'], [fragment(text,UCI[:4]),fragment(alt,['b8c6','f1c4'],start='rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2')], 'analysis')
# The content stream deliberately paints the right column first.
save('columns.pdf', [[(330,750,'3. d4 Bg4'),(330,730,'4. dxe5 Bxf3'),(40,750,'1. e4 e5'),(40,730,'2. Nf3 d6')]], [fragment('1. e4 e5',UCI[:2]),fragment('2. Nf3 d6',UCI[2:4],start=mid),fragment('3. d4 Bg4',UCI[4:6],start='rnbqkb1r/ppp2ppp/3p1n2/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3')], 'PDF columns')
# Correct the third fragment to the Opera position after 2...d6 (the knight is still g8).
column_truth = json.loads((ROOT / 'columns.json').read_text())
column_truth['fragments'][2]['start_fen'] = 'rnbqkbnr/ppp2ppp/3p4/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 0 3'
column_truth['fragments'].append(fragment('4. dxe5 Bxf3',UCI[6:8],start='rn1qkbnr/ppp2ppp/3p4/4p3/3PP1b1/5N2/PPP2PPP/RNBQKB1R w KQkq - 1 4'))
(ROOT / 'columns.json').write_text(json.dumps(column_truth,indent=2)+'\n')
font_grid = '4tMvWlVmT5\n4OoOoOoOo5\n4*+*+*+*+5\n4+*+*+*+*5\n4*+*+*+*+5\n4+*+*+*+*5\n4pPpPpPpP5\n4RnBqKbNr5\nWhite to move'
save('font-diagram.epub',['<pre>'+font_grid+'</pre><p>1. e4 e5</p>'],[fragment('1. e4 e5',UCI[:2],start=diagram_fen)],'Merida font',diagrams=[diagram_fen])
promotion_fen = 'k7/4P3/8/8/8/8/8/4K3 w - - 0 1'
for name,move in [('promotion','e8Q'),('descriptive-promotion','P-K8 (Q)')]:
    text = '1. '+move
    save(name+'.epub',['<p>'+promotion_fen+'</p><p>'+text+'</p>'],[fragment(text,['e7e8q'],start=promotion_fen)],'promotion')
text = '1. e4! an excellent move e5 $1 2. Nf3 text about the centre d6 ±'
save('comments.epub',['<p>'+text+'</p>'],[fragment(text,UCI[:4])],'comments and NAGs')
print('Regenerated legacy fixtures')

# Broad matrix. The independent Rust source replayer never calls woodpeck::books.
# Compile once: cargo build --locked --example books_oracle
import subprocess
import random
from games import GAMES
sources = GAMES + [dict(name=f'Seeded legal walk {i}', seed=0xB00C+i, plies=160) for i in range(4)]
sources += [dict(name='Underpromotion', fen='7k/P7/8/8/8/8/7p/4K3 w - - 0 1', pgn='a8=N h1=N'),
            dict(name='En passant', pgn='e4 a6 e5 d5 exd6'),
            dict(name='Rank disambiguation', fen='7k/8/8/8/8/4R3/8/K3R3 w - - 0 1',pgn='R1e2'),
            dict(name='Full disambiguation',fen='1k6/8/8/8/7Q/8/8/K2Q3Q w - - 0 1',pgn='Qh4e1+')]
oracle = Path(__file__).parents[2] / 'target/debug/examples/books_oracle'
games = json.loads(subprocess.check_output([str(oracle)], input=json.dumps(sources).encode()))
assert len({tuple(g['uci']) for g in games[:len(GAMES)]}) >= 30
assert sum(len(g['uci']) >= 120 for g in games[:len(GAMES)]) >= 1
seen = set()
rng = random.Random(8128)
manifest = []

def descriptive(g, style):
    result = []
    for i,(san,u,fen) in enumerate(zip(g['san'],g['uci'],g['fens'])):
        # Read the source board before each move; this correctly handles ep and promotions.
        squares = {}
        for r,row in enumerate(fen.split()[0].split('/')):
            f = 0
            for c in row:
                if c.isdigit(): f += int(c)
                else: squares[chr(97+f)+str(8-r)] = c; f += 1
        role = squares[u[:2]].upper()
        white = fen.split()[1] == 'w'
        if san.startswith('O-O'):
            token = 'Castles' if san == 'O-O' and style % 2 else san
        else:
            rank = int(u[3]) if white else 9-int(u[3])
            file = ['QR','QKt','QB','Q','K','KB','KKt','KR'][ord(u[2])-97]
            piece = 'Kt' if role == 'N' and style != 2 else role
            capture = 'x' in san
            # Explicit source square preserves intent when several pieces can capture.
            originrank = int(u[1]) if white else 9-int(u[1])
            originfile = ['QR','QKt','QB','Q','K','KB','KKt','KR'][ord(u[0])-97]
            if role == 'N' and u[:2] in ['b1','g1','b8','g8']:
                piece = ('Q' if u[0]=='b' else 'K')+piece
            target = file+str(rank)
            if style == 1 and capture:
                captured = squares.get(u[2:4], 'p').upper()
                target = 'Kt' if captured == 'N' else captured
            token = piece+('x' if capture else '-')+target
            if len(u)==5: token += ' ('+u[4].upper()+')'
            if '+' in san: token += ' ch' if style%2 else '+'
            if '#' in san: token += ' mate'
            if style == 3:
                token = token.replace('Kt','Kt. ').replace('-', ' to ').replace('x',' takes ')
                token = token.replace('KB3', "K. B's 3d.").replace('KKt.', 'K. Kt.').replace('QKt.', 'Q. Kt.')
            elif style == 1: token = token.replace('-', ' - ').replace('x',' x ')
        result.append(token)
    return result

for gi,g in enumerate(games):
    key = (g['fens'][0],tuple(g['uci']))
    if key in seen: continue
    seen.add(key)
    manifest.append(dict(id=gi, name=g['source']['name'], plies=len(g['uci']),seed=g['source'].get('seed')))
    variants = [('san','KQRBN'),('lan',None),('figurine','♔♕♖♗♘'),('pt','RDTBC'),('es','RDTAC'),('de','KDTLS'),('fr','RDTFC'),('nl','KDTLP'),('it','RDTAC'),('pl','KHWGS')]
    variants += [(f'descriptive-{i}',i) for i in range(4)]
    for ni,(name,alphabet) in enumerate(variants):
        if isinstance(alphabet,int): moves = descriptive(g,alphabet)
        elif alphabet is None: moves = [u[:2]+'-'+u[2:4]+('='+u[4].upper() if len(u)==5 else '') for u in g['uci']]
        else: moves = [s.translate(str.maketrans('KQRBN',alphabet)) for s in g['san']]
        # Different whole games and notation/layout pairings, with a reproducible sample.
        layout = rng.randrange(5)
        text = numbered(moves)
        if layout == 1: text = ' '.join(f'{i//2+1}{"." if i%2==0 else "..."} {m}' for i,m in enumerate(moves))
        elif layout == 2: text = ' '.join((f'{i//2+1} ' if i%2==0 else '')+m+(',' if i%2==0 else ';') for i,m in enumerate(moves))
        elif layout == 3: text = ' '.join((f'{i//2+1}. ' if i%2==0 else '')+m+(' {the centre} $1' if i%11==0 else '') for i,m in enumerate(moves))
        elif layout == 4: text = ' '.join((f'{i//2+1}. ' if i%2==0 else '')+m+(' an active move' if i%13==0 else '') for i,m in enumerate(moves))
        text = ' '.join(text.split())
        # All text is one paragraph here; separate layout fixtures below test boundaries.
        start = g['fens'][0]
        heading = '<h1>'+html.escape(g['source']['name'])+'</h1>'
        prefix = '<p>'+start+'</p>' if start != START else ''
        save(f'matrix-{gi:02}-{name}.epub',[heading+prefix+'<p>'+html.escape(text)+'</p>'],[fragment(text,g['uci'],start=start)],name)
        # Keep truth compact so the full materialized matrix stays below the size budget.
        p = ROOT / f'matrix-{gi:02}-{name}.json'
        p.write_text(json.dumps(json.loads(p.read_text()),ensure_ascii=False,separators=(',',':'))+'\n')
    # A full-game PDF per source; actual coordinates exercise column reading order.
    pairs = [f'{i//2+1}. '+ ' '.join(g['san'][i:i+2]) for i in range(0,len(g['san']),2)]
    pages = []; fragments = []
    for offset in range(0,len(pairs),40):
        chunk = pairs[offset:offset+40]; lines = []
        for j,line in enumerate(chunk):
            lines.append((40 if j<20 else 330,750-(j%20)*28,line))
            index = (offset+j)*2
            fragments.append(fragment(line,g['uci'][index:index+2],page=len(pages),start=g['fens'][index]))
        pages.append(lines)
    # FEN-started sources are evaluated in EPUB; PDF mainlines begin at initial position.
    if start == START: save(f'matrix-{gi:02}.pdf',pages,fragments,'PDF two columns')
# These negative controls have a complete zero-move oracle, including false positives.
for i,text in enumerate(["Black's King surveys the board; a5 paper; e4 is a good square.", '1. 1899 2. 1901 Page 64', '1. Bh9 Qz2', '12... Nf3 Nc6']):
    save(f'negative-{i}.epub',['<p>'+text+'</p>'],[fragment(text,[],unresolved=i>0)],'negative')
# Source inventory is not truth JSON and is deliberately human-readable.
(ROOT.parent/'inventory.txt').write_text('\n'.join(f"{g['id']:02} {g['plies']:3} plies {g['name']}" for g in manifest)+'\n')
print(f'Broad matrix: {len(manifest)} unique source records')
# Boundary and branch stress uses separate legal source positions, not Opera continuations.
layout_pages=[]; layout_truth=[]
for gi,g in enumerate(games[:8]):
    if len(g['uci'])<24: continue
    heading='<h1>Boundary sample '+str(gi)+'</h1>'
    # Nested alternatives are independent legal replies, with explicit tree parents.
    alt=g['alternatives'][2]
    replies=json.loads(subprocess.check_output([str(oracle)],input=json.dumps([dict(fen=alt['fen'],seed=gi+71,plies=1)]).encode()))[0]
    other=replies['alternatives'][0]
    if other:
        text='1. '+' '.join(g['san'][:2])+' 2. '+g['san'][2]+' (2. '+alt['san']+' '+replies['san'][0]+' (2... '+other['san']+')) '+g['san'][3]+' 3. '+' '.join(g['san'][4:6])
        moves=g['uci'][:3]+[alt['uci'],replies['uci'][0],other['uci']]+g['uci'][3:6]
        parents=[None,0,1,1,3,3,2,6,7]
        layout_truth.append(fragment(text,moves,page=len(layout_pages),parents=parents));layout_pages.append(heading+'<p>'+html.escape(text)+'</p>')
    # Actual two-column HTML tables and one move per block, retaining the global game context.
    rows=[]
    for i in range(0,12,2):rows.append('<tr><td>'+str(i//2+1)+'. '+g['san'][i]+'</td><td>'+g['san'][i+1]+'</td></tr>')
    text=' '.join(f'{i//2+1}. '+ ' '.join(g['san'][i:i+2]) for i in range(0,12,2))
    layout_truth.append(fragment(text,g['uci'][:12],page=len(layout_pages)));layout_pages.append(heading+'<table>'+''.join(rows)+'</table>')
    # Diagram-independent FEN seeded midgame, including noninitial move numbers.
    text=' '.join(f'{i//2+1}{"." if i%2==0 else "..."} '+g['san'][i] for i in range(20,24))
    layout_truth.append(fragment(text,g['uci'][20:24],page=len(layout_pages),start=g['fens'][20]));layout_pages.append(heading+'<p>'+g['fens'][20]+'</p><p>'+text+'</p>')
    # Line breaks inside move lists and prose between plies.
    text=numbered(g['san'][:12]); lines=text.split(' ')
    layout_truth.append(fragment(text,g['uci'][:12],page=len(layout_pages)));layout_pages.append(heading+'<p>'+'<br/>'.join(lines)+'</p>')
    # One move per paragraph; the annotation covers the whitespace-normalized page range.
    text=' '.join(f'{i//2+1}{"." if i%2==0 else "..."} '+g['san'][i] for i in range(12))
    # Block separators are newlines in imported text: score each move independently.
    parts=[]
    for i in range(12):
        move=f'{i//2+1}{"." if i%2==0 else "..."} '+g['san'][i]
        parts.append('<p>'+move+'</p>');layout_truth.append(fragment(move,[g['uci'][i]],page=len(layout_pages),start=g['fens'][i]))
    layout_pages.append(heading+''.join(parts))
save('boundaries.epub',layout_pages,layout_truth,'layout and nested alternatives')

# A prior-ply analysis jump crosses more than a dozen mainline plies.
g=games[0]
text=numbered(g['san'][:20]); alt=g['alternatives'][3]
analysis='Returning to the earlier position: 2... '+alt['san']
save('prior-ply.epub',['<h1>Prior ply analysis</h1><p>'+text+'</p><p>'+analysis+'</p>'],[fragment(text,g['uci'][:20]),fragment(analysis,[alt['uci']],start=g['fens'][3])],'prior ply analysis')
# EPUB spine boundary within a game, without a heading or an invented reset.
a=numbered(g['san'][:12]); b=' '.join((f'{i//2+1}. ' if i%2==0 else '')+g['san'][i] for i in range(12,24))
save('spine-break.epub',['<h1>Spine continuation</h1><p>'+a+'</p>','<p>'+b+'</p>'],[fragment(a,g['uci'][:12]),fragment(b,g['uci'][12:24],page=1,start=g['fens'][12])],'EPUB page break')
fen='7k/8/8/8/8/8/8/R3K3 w - - 0 1'; text='1. R-Q sq'
save('descriptive-square.epub',['<p>'+fen+'</p><p>'+text+'</p>'],[fragment(text,['a1d1'],start=fen)],'descriptive square')
# Exact old-style source-piece phrasing requested by historical books.
text="1. P-K4 P-K4 2. K. Kt. to B's 3d. Q. Kt. to B's 3d."
save('old-style.epub',['<p>'+text+'</p>'],[fragment(text,['e2e4','e7e5','g1f3','b8c6'])],'old style source qualifier')
# Midgame fragment following a real text diagram, with diagram-specific counters.
parts=g['fens'][20].split();parts[2]='-';parts[3]='-';parts[4]='0';parts[5]='1';fen=' '.join(parts)
rows=[]
for row in parts[0].split('/'):
    expanded=''.join('.'*int(c) if c.isdigit() else c for c in row)
    rows.append(expanded.translate(str.maketrans('KQRBNPkqrbnp','♔♕♖♗♘♙♚♛♜♝♞♟')))
text=numbered(g['san'][20:24]);grid='\n'.join(rows)+'\nWhite to move'
save('midgame-diagram.epub',['<pre>'+grid+'</pre><p>'+text+'</p>'],[fragment(text,g['uci'][20:24],start=fen)],'diagram midgame',diagrams=[fen])

# Diagram images rendered from oracle positions by a separate helper (two looks), followed by
# the next moves. Castling and en passant are not readable from a picture, so samples avoid them.
# Compile once: cargo build --locked --example books_diagrams
import struct, tempfile, zlib
def png(width, height, pixel):
    raw = b''.join(b'\0' + bytes(pixel(x, y) for x in range(width)) for y in range(height))
    chunk = lambda kind, data: struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 0, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')
renders=[]; image_pages=[]; image_truth=[]; image_sources=[]; image_diagrams=[]
for gi,g in enumerate(games):
    if len(image_sources) == 10: break
    k = 14 + gi % 9
    if len(g['uci']) < k+4 or any(s.startswith('O-O') for s in g['san'][k:k+4]): continue
    if any(f.split()[3] != '-' for f in g['fens'][k:k+4]): continue
    parts = g['fens'][k].split(); placement, turn = parts[0], parts[1]
    number = int(parts[5]); start = f'{placement} {turn} - - 0 {number}'
    style = ['screen', 'printed'][len(image_sources) % 2]
    name = f'diagram-{len(image_sources):02}.' + ('png' if style == 'screen' else 'jpg')
    renders.append(dict(file=name, placement=placement, style=style, square=26 + 2 * (len(image_sources) % 5), seed=gi + 101))
    text = ' '.join((f'{(k+i)//2+1}. ' if (k+i) % 2 == 0 else (f'{(k+i)//2+1}... ' if i == 0 else '')) + g['san'][k+i] for i in range(4))
    prose = ['', 'White to play.', 'Black to play.'][0 if len(image_sources) % 3 else (1 if turn == 'w' else 2)]
    image_pages.append(f'<h2>Position {len(image_sources)+1}</h2><p><img src="images/{name}" alt="Chess diagram"/></p>' + (f'<p>{prose}</p>' if prose else '') + '<p>' + text + '</p>')
    image_truth.append(fragment(text, g['uci'][k:k+4], page=len(image_pages)-1, start=start))
    fen = f'{placement} {turn} - - 0 1'
    image_sources.append(dict(asset_suffix='/' + name, fen=fen)); image_diagrams.append(fen)
with tempfile.TemporaryDirectory() as tmp:
    subprocess.run([str(oracle.parent / 'books_diagrams'), tmp], input=json.dumps(renders).encode(), check=True)
    images = {r['file']: (Path(tmp) / r['file']).read_bytes() for r in renders}
# Negative control: a picture that is not a board must stay unrecognized, with its moves unresolved.
images['portrait.png'] = png(160, 200, lambda x, y: (x * 3 + y * 5) % 256 // 2 + 64)
text = '1. e4 e5'
image_pages.append('<h2>Not a diagram</h2><p><img src="images/portrait.png" alt="Portrait"/></p><p>' + text + '</p>')
image_truth.append(fragment(text, [], page=len(image_pages)-1, unresolved=True))
save('image-diagrams.epub', image_pages, image_truth, 'image diagrams', diagrams=image_diagrams, images=images, sources=image_sources)

# Bundle one source per spine page, preserving headings and exact oracle anchors.
# This removes repeated container metadata without shrinking the evaluation sample.
for name,_ in variants:
    paths = sorted(ROOT.glob(f'matrix-*-{name}.epub'))
    pages = []; truths = []
    for path in paths:
        with zipfile.ZipFile(path) as z:
            content = z.read('p0.xhtml').decode().split('<body>',1)[1].split('</body>',1)[0]
        truth = json.loads(path.with_suffix('.json').read_text())
        for f in truth['fragments']: f['page'] = len(pages); truths.append(f)
        pages.append(content)
        path.unlink(); path.with_suffix('.json').unlink()
    save(f'matrix-{name}.epub',pages,truths,name)
for p in ROOT.glob('matrix-*.json'):
    p.write_text(json.dumps(json.loads(p.read_text()),ensure_ascii=False,separators=(',',':'))+'\n')

# Continuations: a game resumed after prose, analysis that jumps back, a black-move
# continuation, a move number two lines reach (must abstain) and a new game that resets.
line = json.loads(subprocess.check_output([str(oracle)], input=json.dumps([dict(name='Opera', pgn=' '.join(SAN[:16]))]).encode()))[0]
assert line['uci'] == UCI[:16]
fens = line['fens']
after_bg4 = fens[6]
pages = ['<h2>Game 1</h2><p>1. e4 e5 2. Nf3 d6 3. d4 Bg4</p><p>White now opens the centre.</p>'
         '<p>4. dxe5 Bxf3 5. Qxf3 dxe5</p><p>Also possible was 4. Nc3 Nf6.</p>'
         '<p>Now 5. h3 is playable in either line.</p><p>6. Bc4 Nf6 7. Qb3</p>'
         '<p>Black has to defend.</p><p>7... Qe7 8. Nc3</p>',
         '<h2>Game 2</h2><p>4. d4 exd4</p>']
truth = [fragment('1. e4 e5 2. Nf3 d6 3. d4 Bg4', UCI[:6]),
         fragment('4. dxe5 Bxf3 5. Qxf3 dxe5', UCI[6:10], start=after_bg4),
         fragment('4. Nc3 Nf6', ['b1c3', 'g8f6'], start=after_bg4),
         fragment('5. h3', [], unresolved=True),
         fragment('6. Bc4 Nf6 7. Qb3', UCI[10:13], start=fens[10]),
         fragment('7... Qe7 8. Nc3', UCI[13:15], start=fens[13]),
         fragment('4. d4 exd4', [], page=1, unresolved=True)]
save('continuations.epub', pages, truth, 'continuations')
# A board printed as text rows ("8 | #R | #Kt| ..."), captioned, with the side to move in prose.
full = json.loads(subprocess.check_output([str(oracle)], input=json.dumps([dict(name='Opera', pgn=' '.join(SAN))]).encode()))[0]
placement = full['fens'][20].split()[0]
names = dict(K='K', Q='Q', R='R', B='B', N='Kt', P='')
rows = ['+' + '-' * 40 + '+']
for rank, row in zip(range(8, 0, -1), placement.split('/')):
    cells = ''.join(' ' * int(c) if c.isdigit() else c for c in row)
    rows.append(f'{rank} |' + '|'.join(f' {("^" if c.isupper() else "#") + names[c.upper()]:<3}' if c != ' ' else '    ' for c in cells) + '|')
    rows.append('|' + '-' * 40 + '|')
rows.append('   a    b    c    d    e    f    g    h')
text = numbered(SAN)[numbered(SAN).index('11. '):].split(' 12.')[0]
assert text == '11. Bxb5+ Nbd7'
save('text-grid.epub', ['<pre>' + '\n'.join(rows) + '</pre><h3>DIAGRAM 1.</h3><p>White to move.</p><p>' + text + '</p>'],
     [fragment(text, UCI[20:22], start=f'{placement} w - - 0 11')], 'text grid', diagrams=[f'{placement} w - - 0 1'])

output_bytes = sum(p.stat().st_size for p in ROOT.iterdir() if p.name not in ['metrics.json','report.md'])
assert output_bytes < 2_000_000, output_bytes
print('Committed fixture bytes:', output_bytes)
