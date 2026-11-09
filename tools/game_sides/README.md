# Game-database side table

`src/books/game_sides.bin` tells the book importer which side was to move when a board
placement occurred in real games. `build.py` builds it from public PGN files only; no book,
page or diagram is read.

Rule: positions with at least 13 men within the first 80 plies, counted once per game. A
placement seen in at least `K` games is stored; with one entry when every game had the same
side to move, with two when both sides occurred. The app takes the side only from
single-entry placements, only when no printed or textual signal names a side, and drops all
table sides for a book in which the table contradicts any side the book prints.

Shipped table (`K = 5`, 3,021,533 bytes, 503,587 entries):

```sh
U=https://database.nikonoel.fr/lichess_elite_
python3 build.py --workers 3 --min-games 5 game_sides.bin \
  ${U}2020-06.zip ${U}2020-09.zip ${U}2020-12.zip
```

1,281,634 games of the Lichess Elite Database (lichess.org standard games filtered by rating,
by nikonoel). The source games are lichess.org database exports, released under CC0.
Requires `python-chess` and `numpy`.
