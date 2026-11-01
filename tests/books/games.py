"""Historical move records, without annotations.
Sources: https://github.com/xydinesh/Games (public.pgn and Capablanca.pgn).
Only historical game facts are reproduced; no source commentary.
"""
GAMES = [{'name': 'Morphy – De Riviere (Paris)',
  'pgn': 'e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Qe7 d4 Bb6 O-O d6 h3 Nf6 Re1 h6 a4 a5 Na3 Nd8 Nc2 Be6 Ne3 Bxc4 '
         'Nxc4 Nd7 Ne3 g6 Nd5 Qe6 Bxh6 f6 Bg7 Rh5 g4 Rxh3 Nxf6+ Nxf6 Ng5 Qd7 Bxf6 Rh4 f3 exd4 cxd4 '
         'Rh6 Kg2 Nf7 Rh1 Nxg5 Rxh6 Nh7 Qh1 Nxf6 Rh8+ Ke7 Rxa8 Bxd4 Qh6 Qc6 Rc1 Qb6 Rxc7+ Ke6 Re8+ '
         'Nxe8 Qxg6+ Ke5 Qf5#'},
 {'name': 'Meek – Morphy (New York)',
  'pgn': 'e4 e5 Nf3 Nc6 d4 exd4 c3 dxc3 Nxc3 Bc5 Bc4 d6 h3 Be6 Bb5 Nge7 Ng5 O-O Qh5 h6 Nf3 Ng6 g4 '
         'Nce5 Nxe5 dxe5 g5 Qd4 Be3 Qb4 Bxc5 Qxb2 O-O Qxc3 Bxf8 Rxf8 Rac1 Qb2 Bc4 Nf4 Qd1 Nxh3+ '
         'Kg2 Nf4+ Kh1 Qb6 gxh6 Bxc4 h7+ Kxh7 Qg4 Qh6+ Kg1 Bxf1 Rxf1 Rd8 a4 Rd6 f3 Rg6 Kf2'},
 {'name': 'Morphy – Lowenthal (London)',
  'pgn': 'e4 e5 Nf3 Nc6 Bc4 Bc5 b4 Bxb4 c3 Bc5 O-O d6 d4 exd4 cxd4 Bb6 d5 Ne5 Nxe5 dxe5 Bb2 Qe7 '
         'Bb5+ Bd7 Bxd7+ Kxd7 Qg4+ f5 Qxf5+ Ke8 Bxe5 Nh6 Qf4 Kd7 Nd2 Rae8 Nc4 Bc5 Rad1 Bd6 Bxd6 '
         'cxd6 Rb1 b6 Rfc1 Qf6 Qe3 Ng4 Nxb6+ axb6 Rc7+ Kd8 Qxb6 Qxf2+ Qxf2 Nxf2 Ra7 Nh3+ gxh3 Kc8 '
         'Kf2'},
 {'name': 'Morphy – Bottin (Paris)',
  'pgn': 'e4 e5 c3 Nf6 d4 Nxe4 dxe5 Bc5 Qg4 Nxf2 Qxg7 Rf8 Bg5 f6 exf6 d5 Be2 Bg4 Bxg4 Nxg4 f7+ '
         'Rxf7 Qxf7+ Kxf7 Bxd8 Nf2 Bh4 Nxh1 Nf3 Nc6 Nbd2 Rg8 Kf1 d4 cxd4 Nxd4 Nxd4 Bxd4 Nf3 Bb6 '
         'Re1 h6 Re7+ Kf8 Ne5 Rg7 Nd7+ Kg8 Rxg7+ Kxg7 Nxb6 axb6 g4 Kg6 Kg2 h5 h3'},
 {'name': 'Paulsen – Morphy (New York)',
  'pgn': 'e4 e5 Nf3 Nc6 Nc3 Nf6 Bb5 Bc5 O-O O-O Nxe5 Re8 Nxc6 dxc6 Bc4 b5 Be2 Nxe4 Nxe4 Rxe4 Bf3 '
         'Re6 c3 Qd3 b4 Bb6 a4 bxa4 Qxa4 Bd7 Ra2 Rae8 Qa6 Qxf3 gxf3 Rg6+ Kh1 Bh3 Rd1 Bg2+ Kg1 '
         'Bxf3+ Kf1 Bg2+ Kg1 Bh3+ Kh1 Bxf2 Qf1 Bxf1 Rxf1 Re2 Ra1 Rh6 d4 Be3'},
 {'name': 'Morphy – Ayers (Mobile)',
  'pgn': 'e4 e5 Nf3 Nc6 Bc4 Bc5 b4 Bxb4 c3 Ba5 d4 d6 Qb3 Qe7 d5 Nd4 Bb5+ c6 Nxd4 exd4 dxc6 Qxe4+ '
         'Kd1 Bg4+ f3 Bxf3+ gxf3 Qxf3+ Kc2 Qe4+ Kb2 Bxc3+ Nxc3 dxc3+ Qxc3 O-O-O Re1 Qd5 cxb7+ Kxb7 '
         'Rb1 Nf6 Bc6+ Qxc6 Ka1+ Kc7 Qa5+ Kc8 Qxa7 Nd7 Bd2'},
 {'name': 'Morphy – Anderssen (Paris)',
  'pgn': 'e4 d5 exd5 Qxd5 Nc3 Qa5 d4 e5 dxe5 Qxe5+ Be2 Bb4 Nf3 Bxc3+ bxc3 Qxc3+ Bd2 Qc5 Rb1 Nc6 '
         'O-O Nf6 Bf4 O-O Bxc7 Nd4 Qxd4 Qxc7 Bd3 Bg4 Ng5 Rfd8 Qb4 Bc8 Rfe1 a5 Qe7 Qxe7 Rxe7 Nd5 '
         'Bxh7+ Kh8 Rxf7 Nc3 Re1 Nxa2 Rf4 Ra6 Bd3'},
 {'name': 'Anderssen – Dufresne (Berlin)',
  'pgn': 'e4 e5 Nf3 Nc6 Bc4 Bc5 b4 Bxb4 c3 Ba5 d4 exd4 O-O d3 Qb3 Qf6 e5 Qg6 Re1 Nge7 Ba3 b5 Qxb5 '
         'Rb8 Qa4 Bb6 Nbd2 Bb7 Ne4 Qf5 Bxd3 Qh5 Nf6+ gxf6 exf6 Rg8 Rad1 Qxf3 Rxe7+ Nxe7 Qxd7+ Kxd7 '
         'Bf5+ Ke8 Bd7+ Kf8 Bxe7#'},
 {'name': 'Anderssen – Dufresne (Berlin)',
  'pgn': 'e4 e5 Nf3 Nc6 Bc4 Bc5 b4 Bxb4 c3 Ba5 d4 exd4 O-O d3 Qb3 Qf6 e5 Qg6 Re1 Nge7 Ba3 b5 Qxb5 '
         'Rb8 Qa4 Bb6 Nbd2 Bb7 Ne4 Qf5 Bxd3 Qh5 Nf6+ gxf6 exf6 Rg8 Rad1 Qxf3 Rxe7+ Nxe7 Qxd7+ Kxd7 '
         'Bf5+ Ke8 Bd7+ Kf8 Bxe7#'},
 {'name': 'Rosanes – Anderssen (Breslau)',
  'pgn': 'e4 e5 f4 exf4 Nf3 g5 h4 g4 Ne5 Nf6 Bc4 d5 exd5 Bd6 d4 Nh5 Bb5+ c6 dxc6 bxc6 Nxc6 Nxc6 '
         'Bxc6+ Kf8 Bxa8 Ng3 Rh2 Bf5 Bd5 Kg7 Nc3 Re8+ Kf2 Qb6 Na4 Qa6 Nc3 Be5 a4 Qf1+ Qxf1 Bxd4+ '
         'Be3 Rxe3 Kg1 Re1#'},
 {'name': 'Anderssen – Kieseritzky (London)',
  'pgn': 'e4 e5 f4 exf4 Bc4 Qh4+ Kf1 b5 Bxb5 Nf6 Nf3 Qh6 d3 Nh5 Nh4 Qg5 Nf5 c6 g4 Nf6 Rg1 cxb5 h4 '
         'Qg6 h5 Qg5 Qf3 Ng8 Bxf4 Qf6 Nc3 Bc5 Nd5 Qxb2 Bd6 Qxa1+ Ke2 Bxg1 e5 Na6 Nxg7+ Kd8 Qf6+ '
         'Nxf6 Be7#'},
 {'name': 'Anderssen – Kierseritzky (London)',
  'pgn': 'e4 e5 f4 exf4 Bc4 Qh4+ Kf1 b5 Bxb5 Nf6 Nf3 Qh6 d3 Nh5 Nh4 Qg5 Nf5 c6 g4 Nf6 Rg1 cxb5 h4 '
         'Qg6 h5 Qg5 Qf3 Ng8 Bxf4 Qf6 Nc3 Bc5 Nd5 Qxb2 Bd6 Bxg1 e5 Qxa1+ Ke2 Na6 Nxg7+ Kd8 Qf6+ '
         'Nxf6 Be7#'},
 {'name': 'Steinitz – Zukertort (match)',
  'pgn': 'e4 e5 Nf3 Nc6 d4 exd4 Nxd4 Nf6 Nc3 Bb4 Nxc6 bxc6 Bd3 d5 exd5 cxd5 O-O O-O Bg5 c6 Ne2 Bd6 '
         'Ng3 h6 Bd2 Ng4 Be2 Qh4 Bxg4 Bxg4 Qc1 Be2 Re1 Ba6 Bc3 f5 Re6 Rad8 Qd2 d4 Ba5 Rd7 Rxd6 '
         'Rxd6 Bb4 Qf6 Rd1 Rd5 Bxf8 Qxf8 Nh5 Qe8 Nf4 Re5 h4 c5 h5 Re4 c3 Qb8 g3 Qe5 Ng6 Qd6 Nf4 d3 '
         'b3 c4 Rb1 Kh7 Kh2 Qb6 Kg1 Bb7 Rb2 Qc6 f3 Qc5+ Qf2 Re1+ Kh2 Qxf2+ Rxf2 Bxf3 g4 Be2 Ng2 d2 '
         'Ne3 cxb3 axb3 Bxg4'},
 {'name': 'Zukertort – Steinitz (World Championship)',
  'pgn': 'd4 d5 c4 e6 Nc3 Nf6 e3 c5 Nf3 Nc6 a3 dxc4 Bxc4 cxd4 exd4 Be7 O-O O-O Be3 Bd7 Qd3 Rc8 '
         'Rac1 Qa5 Ba2 Rfd8 Rfe1 Be8 Bb1 g6 Qe2 Bf8 Red1 Bg7 Ba2 Ne7 Qd2 Qa6 Bg5 Nf5 g4 Nxd4 Nxd4 '
         'e5 Nd5 Rxc1 Qxc1 exd4 Rxd4 Nxd5 Rxd5 Rxd5 Bxd5 Qe2 h3 h6 Bc4 Qf3 Qe3 Qd1+ Kh2 Bc6 Be7 '
         'Be5+ f4 Bxf4+ Qxf4 Qh1+ Kg3 Qg1+ Kh4 Qe1+ Qg3 Qxe7+ g5 Qe4+ Qg4 Qe1+ Qg3 hxg5+ Kg4 Qe4+ '
         'Kxg5 Qxc4 Qb8+ Kh7'},
 {'name': 'Pillsbury – Steinitz (It)',
  'pgn': 'd4 d5 c4 dxc4 Nf3 e6 e3 c5 Nc3 Nc6 Bxc4 Nf6 O-O cxd4 exd4 Be7 Bf4 O-O Rc1 Qb6 Nb5 Ne8 '
         'Re1 Na5 Bd3 Bd7 Nc7 Rc8 Nd5 exd5 Rxe7 Nf6 Ng5 Bg4 Bxh7+ Nxh7 Qxg4 Rxc1+ Bxc1 Nf6 Qd1 Nc6 '
         'Re1 Qxd4 Nf3 Qb6 Bg5 Qxb2 Re2 Qb5 Bxf6 gxf6 Rd2 Rd8 Nh4 d4 Rd3 Ne5 Rb3 Qc6 Rg3+ Kf8 Qd2 '
         'Rc8 Qh6+ Ke7 Nf5+ Kd7 h4 Qc1+ Qxc1 Rxc1+ Kh2 d3'},
 {'name': 'Janowsky – Steinitz (Cologne)',
  'pgn': 'e4 e5 f4 exf4 Bc4 Ne7 Qh5 Ng6 Nc3 Qe7 d4 Nc6 Nf3 Qb4 Qd5 Nd8 a3 Qe7 O-O d6 Qh5 c6 Bd2 '
         'Ne6 Rae1 Qc7 d5 Nd8 e5 dxe5 Nxe5 Bc5+ Kh1 O-O dxc6 Be3 Nf3 Bxd2 Ng5 h6 Qxg6 hxg5 Nd5 '
         'Qxc6 Ne7+ Kh8 Qxg5 Qh6 Qc5 Ne6 Bxe6 Bxe6 Re5 Be3 Qb5 g6 Qxb7 Kg7 Qf3 Rad8 h3 Qh4 Nc6 Bg4 '
         'Qxg4 Qxg4 hxg4 Rh8+ Rh5 gxh5'},
 {'name': 'Lasker – Steinitz (St.Petersburg)',
  'pgn': 'e4 e5 Nf3 Nc6 Bb5 a6 Ba4 d6 d4 Bd7 c3 Nf6 Nbd2 Be7 O-O O-O Re1 Re8 Nf1 Bf8 Ng3 g6 h3 Bg7 '
         'Bc2 Bc8 d5 Ne7 Be3 Rf8 Qd2 Ne8 Bh6 Kh8 Rad1 Ng8 Bxg7+ Nxg7 c4 f5 Qc3 fxe4 Bxe4 Nf6 Qe3 '
         'Nxe4 Nxe4 Rf4 c5 Bf5 Nfg5 Qd7 Qxf4 exf4 Nf6 Ne6 Nxd7 Nxg5 Re7 Kg8 Nf6+ Kf8 Rxc7'},
 {'name': 'Steinitz – Golmayo (Havana)',
  'pgn': 'e4 e5 Nc3 Bc5 f4 d6 Nf3 Nf6 fxe5 dxe5 Nxe5 Qd4 Nd3 Bb6 Qf3 Nc6 Be2 Bg4 Qf4 Bxe2 Kxe2 '
         'O-O-O Ne1 Nb4 a3 Rhe8 axb4 Nxe4 Qf5+ Kb8 Nxe4 Rxe4+ Kd1 Rf4 Qh3 Re8 c3 Qc4 Kc2 Rf2 Nd3 '
         'Rfe2 b3 Qc6 Qf3 Qg6 Rf1 R8e3 Qf5 Qc6 b5 Qd6 Nf4 Re5 Qxf7 R2e4 d3 Re2+ Nxe2 Rxe2+ Bd2'},
 {'name': 'Lasker – Janowsky (New York)',
  'pgn': 'e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O d6 Re1 Bg4 c3 Be7 h3 Bh5 d3 Qd7 Bxc6 Qxc6 Bg5 Bg6 Nbd2 '
         'h6 Bxf6 Bxf6 Nf1 O-O Ne3 Rae8 Qb3 Bd8 Qd5 Qxd5 Nxd5 f5 Nd2 Bf7 Ne3 f4 Nec4 Bf6 a4 Rd8 '
         'Na5 Rb8 Nf3 g5 Nh2 h5 Nc4 Be6 f3 Rfd8 Re2 Kf7 a5 Rg8 Ra4 Rbd8 Rb4 Bc8 b3 Rh8 Nb2 d5 exd5 '
         'Rxd5 Rc4 c6 b4 Bf5 Rd2 Rhd8 Kf2 Rb5 Ke2 Rbd5 Kd1 Ke6 Kc2 Be7 Nf1 c5 bxc5 Bxc5 Ra4 R8d7 '
         'Rd1 Ba7 Ra3 g4 hxg4 hxg4 c4 R5d6 Nd2 Be3 Rh1 gxf3 gxf3 Rg7 Rh2 Bg1 Re2 Rg3 Nd1 Rd7 Rb3 '
         'Rdg7 Nc3 Be3 Nd5 Rg2 Rxe3 fxe3 Rb6+ Kd7 Nxe3 Kc7 Nxf5 Rh7 Nd6 Rhh2 N6e4 Rh1 Kc3 Rc1+ Kb4 '
         'Rd1 Nb3 Rxd3 Rxb7+ Kc8 Rf7 Rb2 Nec5 Rd6 Rf5 Re2 Ne4 Rd3 Rxe5 Rb2 Nec5 Rxf3 Re8+ Kc7 Re6 '
         'Rc2 Nxa6+ Kb7 Nac5+ Ka7 Re7+ Ka8 Na4 Rh2 Nbc5 Kb8 Rb7+ Kc8 a6'},
 {'name': 'Lasker – Salwe (St Petersburg)',
  'pgn': 'e4 e5 Nf3 Nc6 Bb5 d6 d4 Bd7 Nc3 Nf6 O-O Be7 Bg5 exd4 Nxd4 O-O Bxc6 bxc6 Qd3 Re8 Rae1 c5 '
         'Nb3 Ng4 Bxe7 Rxe7 f4 Rb8 h3 Nh6 f5 f6 Nd5 Re8 c4 Nf7 Qc3 Re5 Nd2 c6 Nf4 Qb6 b3 Rbe8 Qg3 '
         'Kh8 Nh5 Rg8 Rf4 Qd8 Nf3 Re7 Rh4 Qe8 Qf2 Rf8 Qd2 Qb8 Kh1 Rfe8 Rg4 Rg8 Rd1 Qb4 Qf2 Qc3 Qh4 '
         'Nh6 Rf4 Nf7 Kh2 Rge8 Qg3 Rg8 Rh4 g5 fxg6 Rxg6 Qf2 f5 Nf4 Rf6 Ne2 Qb2 Rd2 Qa1 Ng3 Kg8 '
         'exf5 Bxf5 Nd4 cxd4 Nxf5 Kf8 Qxd4 Qxd4 Nxd4 Ne5 Rh5 Ref7 c5 dxc5 Rxe5 cxd4 Rxd4 Rf2 Rd8+ '
         'Kg7 Ra5 Rc2 a3 c5 Rc8 Rb2 Rb5 Rff2 Rb7+ Kg6 Rc6+ Rf6 Rxc5 Ra6 a4 Rf6 Rc3 a6 Rg3+ Kh6 '
         'Rgg7'},
 {'name': 'Janowsky – Lasker (St.Petersburg)',
  'pgn': 'd4 d5 Nf3 c5 c4 e6 e3 Nc6 Bd3 Nf6 O-O Bd6 b3 O-O Bb2 b6 Nbd2 Bb7 Ne5 Qe7 a3 Rad8 Qc2 '
         'dxc4 Ndxc4 cxd4 exd4 Rc8 Qe2 Bb8 f4 Nd5 Rae1 f5 Qd2 Nxe5 Nxe5 a6 Bb1 Bd6 Nc4 b5 Na5 Ba8 '
         'b4 Nb6 Nb3 Bd5 Nc5 Nc4 Qc3 Rf6 Bc1 a5 Rf2 axb4 axb4 Ra8 Ba2 Qf7 Bxc4 Bxc4 Bb2 Rg6 Ra1 '
         'Rxa1+ Bxa1 Qc7 Qe3 Rg4 g3 g5 d5 Bxd5 Qd4 gxf4 Qh8+ Kf7 Qxh7+ Ke8 Qh8+ Bf8 Be5 Qf7 Rxf4 '
         'Rxf4 Bxf4 Qg7 Qh5+ Kd8 Bg5+ Kc7 Bf4+ Bd6 Bxd6+ Kxd6 Qh4 Qa1+ Kf2 Qb2+ Ke1 Qc1+ Ke2 Bc4+ '
         'Kf2 Qd2+ Kf3 Bd5+'},
 {'name': 'Lasker – Dus-Chotimirski (Moscow)',
  'pgn': 'e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O Be7 Re1 b5 Bb3 d6 c3 Na5 Bc2 c5 d3 O-O Nbd2 Re8 Nf1 Qc7 '
         'Bg5 Rb8 a3 Be6 h3 h6 Bd2 Nc6 Qe2 Bf8 a4 b4 c4 Nd7 Be3 g6 N3d2 f5 exf5 gxf5 Qh5 Nf6 Qg6+ '
         'Qg7 Qxg7+ Bxg7 f4 Nd7 Rad1 Bf7 Nb3 exf4 Bxf4 Nd4 Nxd4 Bxd4+ Kh2 b3 Rxe8+ Bxe8 Bb1 Ne5 '
         'Ng3 Bg6 Rd2 Rb4 Ne2 Rxa4 Nxd4 cxd4 c5 Ra1 cxd6 Nd7 Rd1 Bf7 Re1 a5 Re7 Nc5 Be5 Ra4 Rc7 '
         'Ne6 d7 Rb4 Rc8+ Kh7 Rh8+ Kg6 Re8 Rb6 Bf4 Nxf4 d8=Q Rc6 Re7'},
 {'name': 'Tartakower – Lasker (New York)',
  'pgn': 'c4 e5 a3 Nf6 e3 Be7 Qc2 O-O Nc3 d6 Nf3 Re8 Be2 Bf8 O-O Nc6 d4 Bg4 d5 Ne7 h3 Bd7 Nh2 Qc8 '
         'e4 Ng6 f4 exf4 Bxf4 Nxf4 Rxf4 Be7 Raf1 Rf8 Qd3 Be8 Qg3 Qd8 Nd1 Nd7 Ne3 Bg5 Rg4 f6 Qf2 h5 '
         'Rg3 h4 Rg4 Bh5 Nf5 Bxg4 Nxg4 Qe8 Bf3 Ne5 Nxe5 Qxe5 Nxh4 Bxh4 Qxh4 f5 exf5 Rxf5 Re1 Qxb2 '
         'Bg4 Qd4+ Kh2 Raf8 Qe7 Qf4+ Kh1 Re5 Rxe5 dxe5 Qxc7 e4 Qe7 Qf6 Qxb7 Qa1+ Kh2 Qe5+ Kg1 Rb8 '
         'Qd7 Rb1+ Kf2 e3+ Ke2 Rb2+ Ke1 Qc3+ Kf1 Qc1+ Bd1 Qxd1#'},
 {'name': 'Pillsbury – Lasker (Nuremberg)',
  'pgn': 'e4 e6 d4 d5 Nc3 Nf6 e5 Nfd7 f4 c5 dxc5 Nc6 a3 Nxc5 b4 Nd7 Bd3 a5 b5 Ncb8 Nf3 Nc5 Be3 '
         'Nbd7 O-O g6 Ne2 Be7 Qe1 Nb6 Nfd4 Bd7 Qf2 Nba4 Rab1 h5 b6 Nxd3 cxd3 Bxa3 f5 gxf5 Nf4 h4 '
         'Ra1 Be7 Rxa4 Bxa4 Ndxe6 fxe6 Nxe6 Bd7 Nxd8 Rxd8 Bc5 Rc8 Bxe7 Kxe7 Qe3 Rc6 Qg5+ Kf7 Rc1 '
         'Rxc1+ Qxc1 Rc8 Qe1 h3 gxh3 Rg8+ Kf2 a4 Qb4 Rg6 Kf3 a3 Qxa3 Rxb6 Qc5 Re6 Qc7 Ke7 Kf4 b6 '
         'h4 Rc6 Qb8 Be8 Kxf5 Rh6 Qc7+ Kf8 Qd8 b5 e6 b4 Qd6+'},
 {'name': 'Reti – Rubinstein (Karlsbad)',
  'pgn': 'Nf3 d5 g3 Nf6 Bg2 g6 c4 d4 d3 Bg7 b4 O-O Nbd2 c5 Nb3 cxb4 Bb2 Nc6 Nbxd4 Nxd4 Bxd4 b6 a3 '
         'Bb7 Bb2 bxa3 Rxa3 Qc7 Qa1 Ne8 Bxg7 Nxg7 O-O Ne6 Rb1 Bc6 d4 Be4 Rd1 a5 d5 Nc5 Nd4 Bxg2 '
         'Kxg2 Rfd8 Nc6 Rd6 Re3 Re8 Qe5 f6 Qb2 e5 Qb5 Kf7 Rb1 Nd7 f3 Rc8 Rd3 e4 fxe4 Ne5 Qxb6 Nxc6 '
         'c5 Rd7 dxc6 Rxd3 Qxc7+ Rxc7 exd3 Rxc6 Rb7+ Ke8 d4 Ra6 Rb6 Ra8 Rxf6 a4 Rf2 a3 Ra2 Kd7 d5 '
         'g5 Kf3 Ra4 Ke3 h5 h4 gxh4 gxh4 Ke7 Kf4 Kd7 Kf5'},
 {'name': 'Rubinstein – Nimzowitsch (Karlsbad)',
  'pgn': 'd4 d5 Nf3 e6 c4 c5 cxd5 exd5 Nc3 Nc6 Bf4 cxd4 Nxd4 Bb4 e3 Nf6 Nxc6 bxc6 Bd3 O-O O-O Bd6 '
         'Bg3 Bxg3 hxg3 c5 Rc1 Be6 Qa4 Qb6 Qa3 c4 Be2 a5 Rfd1 Qb4 Rd4 Rfd8 Rcd1 Rd7 Bf3 Rad8 Nb1 '
         'Rb8 R1d2 Qxa3 Nxa3 Kf8 e4 dxe4 Rxd7 Nxd7 Bxe4 Nc5 Rd4 Nxe4 Rxe4 Rxb2 Nxc4 Rb4 Nd6 Rxe4 '
         'Nxe4 Bxa2 Nc3 Bc4 f4 Ke7 Kf2 Kd6 Ke3 Kc5 g4 Kb4 Kd4 Bb3 g5 a4 Nb1 Be6 g3 Kb3 Nc3 a3 Kd3 '
         'g6 Kd4 Kc2'},
 {'name': 'Capablanca – Rubinstein (Berlin)',
  'pgn': 'd4 d5 Nf3 c5 dxc5 e6 e4 Bxc5 exd5 exd5 Bb5+ Nc6 O-O Nge7 Nbd2 O-O Nb3 Bb6 Re1 Bg4 Bd3 '
         'Ng6 h3 Bxf3 Qxf3 Nce5 Qf5 Nxd3 Qxd3 d4 Bd2 Qf6 Re4 Rad8 Rae1 Qc6 g3 Rfe8 Ba5 Rxe4 Qxe4 '
         'Nf8 Qxc6 bxc6 Re7 Rd5 Bxb6 axb6 Rb7 Nd7 Rc7 Rd6 Rc8+ Nf8 Nd2 c5 Nc4 Re6 Rb8 Re1+ Kg2 g5 '
         'a4 Ra1 Nxb6 Kg7 Rc8 Ne6 Nd7 Rxa4 Nxc5 Rb4 Nd3 Rb5 Kf3 h6 b4 h5 g4 hxg4+ hxg4 f6 Rc4 Kf7 '
         'Nc5 Nd8 Nb3'},
 {'name': 'Rubinstein – Nimzowitsch (Berlin)',
  'pgn': 'd4 Nf6 c4 e6 Nc3 Bb4 Qc2 d6 e3 c5 Bd3 Nc6 Nge2 e5 d5 Bxc3+ Qxc3 Ne7 Qc2 O-O O-O Ng6 Ng3 '
         'Re8 f3 Bd7 Bd2 a6 h3 b5 b3 Qb6 Kh2 a5 Rab1 b4 f4 exf4 exf4 Nf8 Bc1 Qd8 Qf2 a4 Bb2 Ng6 '
         'Rbd1 axb3 axb3 Ra7 Rde1 Rxe1 Rxe1 Nf8 Bxf6 Qxf6 Ne4 Qh6 f5 Ra3 Rb1 Ra6 g4 f6 Kg3 Bc8 Re1 '
         'Bb7 Qe2 Nd7 Nxd6 Rxd6 Qe8+ Nf8 Re7 g6 Qf7+ Kh8 Re8 Rd8 Qxf6+ Kg8 Qe6+ Kg7 f6+'},
 {'name': 'Rubinstein – Capablanca (San Sebastian)',
  'pgn': 'd4 d5 Nf3 c5 c4 e6 cxd5 exd5 Nc3 Nc6 g3 Be6 Bg2 Be7 O-O Rc8 dxc5 Bxc5 Ng5 Nf6 Nxe6 fxe6 '
         'Bh3 Qe7 Bg5 O-O Bxf6 Qxf6 Nxd5 Qh6 Kg2 Rcd8 Qc1 exd5 Qxc5 Qd2 Qb5 Nd4 Qd3 Qxd3 exd3 Rfe8 '
         'Bg4 Rd6 Rfe1 Rxe1 Rxe1 Rb6 Re5 Rxb2 Rxd5 Nc6 Be6+ Kf8 Rf5+ Ke8 Bf7+ Kd7 Bc4 a6 Rf7+ Kd6 '
         'Rxg7 b5 Bg8 a5 Rxh7 a4 h4 b4 Rh6+ Kc5 Rh5+ Kb6 Bd5 b3 axb3 a3 Bxc6 Rxb3 Bd5 a2 Rh6+'},
 {'name': 'Thomas – Rubinstein (Hastings <ENG>)',
  'pgn': 'e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 Qe2 b5 Bb3 Bc5 c3 O-O O-O d6 d3 Ne7 Bg5 Ng6 Nh4 Nxh4 Bxh4 '
         'h6 Kh1 g5 Bg3 Kg7 Nd2 Qe7 Bc2 Bd7 Rfe1 Rae8 a4 Rh8 axb5 axb5 Nf1 h5 f3 h4 Bf2 Bxf2 Qxf2 '
         'g4 fxg4 Nxg4 Qf3 h3 g3 Qg5 Ne3 Ra8 Nxg4 Bxg4 Qf2 Rxa1 Rxa1 Ra8 Rb1 b4 Bb3 f6 c4 f5 Kg1 '
         'fxe4 Rf1 e3 Qf7+ Kh8 Qd5 c6 Qxc6 Rc8 Qe4 e2 Re1 d5 cxd5 Rc1'},
 {'name': 'Capablanca, Jose Raul – Lasker, Emanuel (1914.??.??)',
  'pgn': 'e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O Nxe4 d4 b5 Bb3 d5 dxe5 Be6 Nbd2 Nc5 c3 d4 cxd4 Nxd4 '
         'Nxd4 Qxd4 Bxe6 Nxe6 Qf3 Rd8 a4 Qd5 Qxd5 Rxd5 axb5 axb5 Ra8+ Nd8 Ne4 Rxe5 Rd1 Be7 f3 Rf5 '
         'Rc8 O-O Rxc7 Bb4 Be3 Ne6 Rcd7 Rc8 R1d5 Rxd5 Rxd5 Rc2 b3 Rb2 Rxb5 Rxb3 Bd2 Bc5+ Rxc5 Nxc5 '
         'Nxc5 Rb2 Be3 Re2 Bf2 f6 Kf1 Ra2 g4 Kf7 Ne4 h6 Kg2 Ra3 f4 Rb3 Ng3 Ra3 Nf1 Rd3 Ne3 Rc3 Kf3 '
         'Ra3 f5 Ra2 Nd5 Rb2 Nf4 Ra2 h4 Ra5 Bd4 Ra3+ Be3 Ra5 Nh5 Ra4 Ng3 Kg8 Ne4 Kf7 Bd2 Ra1 Bc3 '
         'Rf1+ Nf2 Rc1 Bd4 Re1 Ne4 Rf1+ Bf2 Ra1 Kf4 Ra4 Bc5 Rc4 Kf3 Rc1 Bf2 Ra1 Kf4 Ra4 Kf3 Ra3+ '
         'Be3 Ra5 Nc5 Ra1 Ne6 Ra3 Ke4 Ra4+ Bd4 Rb4 Kd3 Rb3+ Ke4 Rb4 Kd5 Rb1 g5 hxg5 hxg5 fxg5 '
         'Nxg5+ Kg8 Ne6 Rd1 Ke4 Kf7 Ng5+ Kg8 Ke5 Re1+ Kf4 Rf1+ Kg4 Rd1 Nf3 Rf1 Be5 Kf7 Kf4 Kg8 Ke4 '
         'Rd1 Ng5 Re1+ Kd5 Rd1+ Ke6 Re1 Nh3 Rb1 Nf4 Rb6+ Ke7 Rb5 Ng6 Rb6 Bd6 Ra6 Ke6 Rb6 Ne7+ Kh7 '
         'Nc8 Ra6 Ne7 Rb6 Nd5 Ra6 Nc3 Kg8 Ne4 Rb6'},
 {'name': 'Spielmann, Rudolf – Capablanca, Jose Raul (1925.??.??)',
  'pgn': 'e4 c6 d4 d5 exd5 cxd5 Bd3 Nf6 Bf4 Bg4 Nf3 Nc6 c3 e6 Nbd2 Bd6 Bxd6 Qxd6 h3 Bh5 Qe2 a6 O-O '
         'O-O Qe3 Bxf3 Nxf3 Nd7 Rfe1 b5 a3 Rab8 b4 Nb6 Nd2 Nc4 Bxc4 bxc4 a4 Rfe8 Nf3 f6 Rad1 Kf7 '
         'g3 Ne7 Kg2 g5 Nh2 h5 Rd2 Nf5 Qf3 Rh8 Rde2 Rbe8 Kg1 Qc6 Ra2 h4 Kg2 Kg6 Ng4 Rhf8 Rae2 Qd7 '
         'b5 hxg3 fxg3 axb5 axb5 Nd6 Qxf6+ Rxf6 Ne5+ Kg7 Nxd7 Rf7 Nc5 Nxb5 Nxe6+ Kh6 Nf4 Rxe2+ '
         'Nxe2 Re7 Kf2 Nd6 Ra1 Ne4+ Kf3 Nd2+ Kf2 Rf7+ Kg2 Ne4 Rf1 Ra7 Rc1 Ra2 Kf3 Kh5 Ke3 Rd2 Kf3 '
         'Rd3+ Kg2 Re3 Rc2 Kg6 Ra2 Kf5 Rc2 Ke6 Kh2 Rf3 Kg2 Rf2+ Kg1 Kf5 Rb2 Rf3 Kg2 g4 h4 Re3 Rc2 '
         'Ke6 Kh2 Rf3 Kg2 Rf2+ Kg1 Kf6 Ra2 Rf3 Kg2 Nxc3 Nxc3 Rxc3 Ra5 Ke6 h5 Rf3 Ra6+ Kf7 Rd6 Rf5 '
         'h6 Kg8 Rd7 Kh8 Kg1 c3 Rc7 Rf3 Kg2 Rd3 Kf2 Rf3+ Kg2 Re3 Kf2 Rd3 Rc5 Kh7 Rxd5 Kxh6 Rc5 Kg6 '
         'Ke2 Rxg3 Kf2 Rh3 Kg2 Rd3 Kh2 Kf6 Kg2 Ke6 Kh2 Kd6 Kg2 Rd2+ Kg3 c2 Kh4 Rg2 Kg5 Rg1 Rxc2 '
         'Kd5 Kf4 Kxd4 Rd2+'},
 {'name': 'Bernstein, Jacob – Capablanca, Jose Raul (1915.??.??)',
  'pgn': 'e4 e5 Nf3 Nc6 Nc3 Nf6 Bb5 Nd4 Bc4 Bc5 d3 d6 Bg5 Bg4 Nd5 c6 Nxf6+ gxf6 Be3 Qb6 Bxd4 Bxd4 '
         'O-O d5 exd5 O-O-O c3 cxd5 cxd4 dxc4 Rc1 Kb8 Rxc4 exd4 Qd2 Ka8 Qf4 Be6 Rxd4 Rxd4 Qxd4 '
         'Qxd4 Nxd4 Bxa2 Ra1 Rd8 Rxa2 Rxd4 Ra3 Rb4 Rc3 a5 Rc2 Rb3 Rd2 b5 Kf1 Kb7 Ke2 Kc6 Rc2+ Kd5 '
         'Ke3 a4 Rd2 b4 Rc2 a3 bxa3 bxa3 Rc8 a2 Ra8 Rb2 g4 Kc5 d4+ Kb4 Ke4 Rxf2 d5 Kb3 d6 Rd2 Rb8+ '
         'Kc2 Ra8 Kb1 Rb8+ Kc2 Ra8 Kb3 Rb8+ Kc4 Ra8 Rd4+ Kf3 Kb3 Rb8+ Rb4 Rxb4+ Kxb4 d7 a1=Q d8=Q '
         'Qc3+ Kf4 Qe5+ Kf3 Kc4 Qc8+ Kd4 Qf8 Qe3+ Kg2 Qd2+ Kf3 Qc3+ Kg2 Ke4 Qa8+ Kf4 Qa4+ Ke3 Qa7+ '
         'Qd4 Qa3+ Kf4 Qf3+ Kg5 Qf5+ Kh6 h4 Kg7 Qf3 Qe5 Qd3 f5 gxf5 Kf6 Qd8+ Kxf5 Qg5+ Ke4 Qg4+ '
         'Qf4 Qe2+ Kd4 Qb2+ Kd5 Qb5+ Ke6 Qc6+ Ke7 Qb7+ Kf6 Qb2+ Kg6 Qb1+ f5 Qb3 Qg4+ Kf1 h6 Qe6+ '
         'Kh5 Qe7 Qf3+ Kg1 Kg4 Qd7 Kh3 Qd2 Qg3+ Kf1 f4 Qd7+ Qg4'},
 {'name': 'Alekhine, Alexander – Capablanca, Jose Raul (1927.??.??)',
  'pgn': 'd4 Nf6 c4 e6 Nc3 d5 Bg5 Nbd7 e3 Be7 Nf3 O-O Rc1 c6 Bd3 dxc4 Bxc4 Nd5 Bxe7 Qxe7 Ne4 N5f6 '
         'Ng3 Qb4+ Qd2 Qxd2+ Kxd2 Rd8 Rhd1 b6 e4 Bb7 e5 Ne8 Ke3 Kf8 Ng5 h6 N5e4 Ke7 f4 f5 Nc3 Nc7 '
         'Nge2 g5 h4 g4 Ng3 a5 Bb3 Rac8 a3 Rf8 Rd2 Ba8 Rdc2 c5 dxc5 Nxc5 Na4 N7a6 Bxe6 Kxe6 Nxb6 '
         'Rb8 Nxa8 Rb3+ Rc3 Rxc3+ bxc3 Rxa8 Rd1 Rf8 Rd6+ Ke7 Rxh6 Nc7 Rh7+ Kd8 c4 N7e6 Ra7 Nc7 '
         'Rxa5 N5e6 h5 Kd7 h6 Nxf4 Kxf4 Ne6+ Ke3 f4+ Kf2 fxg3+ Kxg3 Rh8 Rd5+ Ke7 c5 Rxh6 c6 Nf8 '
         'Rc5 Kd8 Kxg4 Rg6+ Kf3 Kc7 g4 Ne6 Rd5 Nd8 Rc5 Ne6 Rd5 Nf8 Ra5 Rxc6 Ke4 Rc1 Ra7+ Kc6 Ra6+ '
         'Kd7 Ra7+ Ke6 Ra6+ Ke7 a4 Nd7 Rh6 Re1+ Kd4 Nxe5 a5 Nxg4 Rh7+ Kd6 a6 Ra1 a7 Nf6 Rb7 Nd7 '
         'Rb2 Rxa7 Rd2 Nc5 Kc4+ Kc6 Rh2 Ra4+ Kc3 Rg4 Kd2 Rg3 Rh5 Kb5 Ke2 Kc4 Rh4+ Kc3 Kf2 Rd3 Rf4 '
         'Kd2 Kg2 Rd5 Kf3 Kd3'},
 {'name': 'Capablanca, Jose Raul – Kostic, Boris (1919.??.??)',
  'pgn': 'e4 e5 Nf3 Nf6 Nxe5 d6 Nf3 Nxe4 Qe2 Qe7 d3 Nf6 Bg5 Qxe2+ Bxe2 Be7 Nc3 Bd7 O-O O-O Rfe1 '
         'Nc6 d4 Rfe8 Bb5 a6 Ba4 b5 Bb3 Na5 Re3 c6 Rae1 Kf8 Bf4 Nb7 h3 h6 Bh2 Bd8 Rxe8+ Bxe8 a4 c5 '
         'Ne4 Nxe4 Bd5 Ra7 Bxe4 Be7 axb5 axb5 dxc5 dxc5 Bb8 Ra8 Bg3 Ra7 Bb8 Ra8 Bg3 Ra7 Ne5 Nd8 b3 '
         'Ne6 Bd5 Nd4 c3 Nf5 Bh2 b4 g4 Nd6 c4 Ra3 Re3 Nc8 Bb7 Na7 Bd5 f6 Nf3 Nc6 Nh4 Nd4 Nf5 Nxf5 '
         'gxf5 Bd7 Be4 Ra6 Rd3 Bc6 Bxc6 Rxc6 Kg2 Ra6 Kf3 Ra2 Bg3 Ke8 Bf4 Ra6 Be3 Ra1 Kg4 Ra7 Kh5 '
         'Kf7 Rd5 Ra3 Rd7 Ke8 Rd3 Kf7 h4 Ra7 Rd5 Ra5 Rd7 Ke8 Rd3 Kf7 Rd5 Ra3 Bxc5 Bxc5 Rxc5 Rxb3 '
         'Rc7+ Kf8 Kg6 Rf3 Rf7+ Ke8 Rxg7 Rf4 h5 Rxc4 Kxh6 Kf8 Rb7 Rg4 f3 Rg5 Rxb4 Kf7 Rg4 Rxf5 f4 '
         'Ra5 Rg7+ Kf8 Rb7 f5 Kg6 Ra6+ Kxf5 Ra5+ Kg4 Ra6 Kg5 Rc6 f5 Kg8 f6 Rc8 Rg7+ Kf8 h6'},
 {'name': 'Capablanca, Jose Raul – Janowsky, Dawid Markelowicz (1916.??.??)',
  'pgn': 'd4 Nf6 Nf3 d5 c4 c6 Nc3 dxc4 e3 Bg4 Bxc4 e6 h3 Bh5 O-O Be7 Qb3 Qb6 Ne5 Nbd7 Qxb6 axb6 '
         'Nxd7 Kxd7 Bd2 b5 Bd3 Bg6 Bxg6 hxg6 Rfc1 Ra6 a3 Rha8 Na2 Nd5 Rab1 f5 Kf1 Ra4 Nc3 Rc4 b3 '
         'Rxc3 Bxc3 Rxa3 Be1 Bf6 Ke2 e5 dxe5 Bxe5 f4 Bd6 Bc3 Ra2+ Kf3 Bc5 Rd1 Ke6 Rd3 Bf8 Be5 b4 '
         'Rc1 g5 g4 g6 e4 fxe4+ Kxe4 Re2+ Kf3 Rh2 Kg3 Re2 h4 gxh4+ Kxh4 Be7+ Kg3 g5 Kf3 Rh2 Re1 '
         'Rh3+ Ke4 Rh4 fxg5 Bxg5 Kf3 Rh3+ Bg3+ Kd7 Kg2 Rh7 Re5 Bf6 Rexd5+ cxd5 Rxd5+ Ke8 Rb5 Rd7 '
         'Rxb4 Kf7 Rb6 Bd4 Rd6 Rxd6 Bxd6 Kg6 Kf3 Bf6 Bf4 Kf7 Ke4 Ke6 Be3 Be7 g5 Bd8 Kf4 Bc7+ Kg4 '
         'Be5 Kh5 Kf7 Kh6 Kg8 Bb6 Bc3 Kg6 Bd2 Kf6 Bc3+ Ke6 Bd2 g6 Bc3 Kd5 Bd2 Bd4 b5 Ke4 b4 Be3 '
         'Bc3 Kd3 Be1 Bd2 Bf2 Ke4 Bc5 Kd5 Be7 Kc4 Kg7 Bxb4 Bd8 Bc3+ Kxg6 b4 Kf5 Kd5'}]
