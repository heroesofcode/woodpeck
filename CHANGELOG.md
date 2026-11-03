# Changelog

## [0.5.0](https://github.com/heroesofcode/woodpeck/compare/v0.4.0...v0.5.0) (2026-11-03)


### Features

* **books:** read diagram images with two small embedded CNNs ([0bcec3e](https://github.com/heroesofcode/woodpeck/commit/0bcec3e743b42d7a9ef8c23b5c99f1ff88340abb))
* **books:** share diagram material checks and add an agreeing recognizer ([d0f6b4e](https://github.com/heroesofcode/woodpeck/commit/d0f6b4ef5adc2e278827fdadec93a66027d4092b))


### Bug fixes

* **books:** do not hang moves from an earlier root on a later fragment start ([6aaa145](https://github.com/heroesofcode/woodpeck/commit/6aaa1451244aaeead8e085302ff41eeded200b68))
* **books:** keep the line when a rewind cannot hang from the fragment ([a56341c](https://github.com/heroesofcode/woodpeck/commit/a56341cb98354f20e741706edd7521eec6175302))
* **books:** keep the restored line when a later number follows a rejected rewind ([c525594](https://github.com/heroesofcode/woodpeck/commit/c5255947ae0d72c933e31dcc5eb419dd105ba902))
* **books:** read a figurine printed with a space before its square as one move ([5b07f8a](https://github.com/heroesofcode/woodpeck/commit/5b07f8a3f396c234531b6620019cbe3f998a8696))
* **books:** read a move number printed with a Unicode ellipsis as Black's move ([ec30195](https://github.com/heroesofcode/woodpeck/commit/ec301957c946c20d27103a5aae577e4dc354d56e))


### Refactoring

* **books:** satisfy pedantic lints in the diagram classifier ([bfa2561](https://github.com/heroesofcode/woodpeck/commit/bfa2561ba1a4d67cedee247605b41075d3c95a2e))

## [0.4.0](https://github.com/heroesofcode/woodpeck/compare/v0.3.0...v0.4.0) (2026-11-02)


### Features

* **app:** add a library and reader for imported chess books ([2be01c5](https://github.com/heroesofcode/woodpeck/commit/2be01c55a709244c683f5f5c1dad39a1d4ae1acf))
* **books:** choose books with the system file dialog ([f4606c8](https://github.com/heroesofcode/woodpeck/commit/f4606c89080aeb41c49d454e333444f7b41f7298))
* **books:** give the reader a type scale, contents and adaptive layout ([3535cce](https://github.com/heroesofcode/woodpeck/commit/3535cceace45bf267b97fd95bf09a0aee24da6e7))
* **books:** import EPUB and PDF chess books into validated move trees ([7bd1ba1](https://github.com/heroesofcode/woodpeck/commit/7bd1ba12b7c1885dcb9119583338133841bcc7a1))
* **books:** keep heading levels and reflow PDF paragraphs ([7e2e4a8](https://github.com/heroesofcode/woodpeck/commit/7e2e4a8a5b1e9c1263170ca97f50b0c56e8d2a86))
* **books:** place PDF pictures in the text and follow games through unread boards ([1711dfe](https://github.com/heroesofcode/woodpeck/commit/1711dfe969c9e35063a07c7d06edb6e3908d861b))
* **books:** recognize diagram images with bundled prototypes ([889271d](https://github.com/heroesofcode/woodpeck/commit/889271da14a35520b5122f38ceea961244378cdb))
* **books:** say what an import found when the book opens ([c8758f1](https://github.com/heroesofcode/woodpeck/commit/c8758f1727d315082c0490eeb6439bfaf4dc26eb))
* **books:** seed positions from recognized diagram images ([c090aa0](https://github.com/heroesofcode/woodpeck/commit/c090aa092a9f18630b160e7137750b9ec40845f8))
* **books:** show book covers in a library grid ([7d6ebaf](https://github.com/heroesofcode/woodpeck/commit/7d6ebafbda9a8340e89ca26a6f5723a33f446287))


### Bug fixes

* **books:** await the file dialog off the UI thread ([f13f350](https://github.com/heroesofcode/woodpeck/commit/f13f350a3739c62c995635e64026f54f7054da9a))
* **books:** ignore asides and failed numbers when placing diagram images ([e8f28b4](https://github.com/heroesofcode/woodpeck/commit/e8f28b418cca622ccd7a2393f3bd81647331136d))
* **books:** invalidate the position at nested images in source order ([4c92299](https://github.com/heroesofcode/woodpeck/commit/4c922991e07ae1e52590e44ef39180eec83271f8))
* **books:** keep diagram positions when a line restarts at move one ([cee03b1](https://github.com/heroesofcode/woodpeck/commit/cee03b1547f80b17ba0789e1f95b5d047a6d22b8))
* **books:** keep plain knights wingless and read PGN headers split across paragraphs ([43d1342](https://github.com/heroesofcode/woodpeck/commit/43d1342a19848fee5e47b4af0632f817cf3482c9))
* **books:** only treat diagram captions as captions and keep move one for black-to-move diagrams ([eeab109](https://github.com/heroesofcode/woodpeck/commit/eeab109a4504fb8e7797ec20b59e3ca5f1fb97ce))
* **books:** preserve confirmed context across intervening prose ([0a80e03](https://github.com/heroesofcode/woodpeck/commit/0a80e03a3e81743ead29a2e96651c2b442dc86ed))
* **books:** read PGN headers and more old descriptive notation ([5d52146](https://github.com/heroesofcode/woodpeck/commit/5d52146375343e40b426e39851893eb37b00bac2))
* **books:** resolve lines that resume a game or start after a diagram ([a961c9a](https://github.com/heroesofcode/woodpeck/commit/a961c9a4484236cf8e806ca76c8f6e4907da718d))
* **books:** retain verified positions through image captions ([399a0d3](https://github.com/heroesofcode/woodpeck/commit/399a0d3e3ad740452b4d0ec3e85ea946136308a7))
* **reader:** keep boards readable and controls reachable in small windows ([7b9e1af](https://github.com/heroesofcode/woodpeck/commit/7b9e1af5ece8936381650c979c3d398464581229))
* **reader:** show no board for passages without a verified position ([634d795](https://github.com/heroesofcode/woodpeck/commit/634d7956efc9011b3774bc13598d38c60a521def))

## [0.3.0](https://github.com/heroesofcode/woodpeck/compare/v0.2.0...v0.3.0) (2026-11-01)


### Features

* **board:** draw arrows and highlight squares with right-click ([7c518b4](https://github.com/heroesofcode/woodpeck/commit/7c518b4eb661ccc804243c50c5822d615f22dcf1))
* responsive training layout and sound effects ([2f6803c](https://github.com/heroesofcode/woodpeck/commit/2f6803c8f4c7449e09560b96a6ed443f793c9685))


### Bug fixes

* **deps:** upgrade zstd to 0.14.3 to clear RustSec advisories ([d52789f](https://github.com/heroesofcode/woodpeck/commit/d52789f63f51b2866052995d8c9eb7908fec4d5c))
* keep the active solve across sync status changes ([b11c495](https://github.com/heroesofcode/woodpeck/commit/b11c495ebf6d7f459fa702f9cb1158612904f2bd))
* **settings:** keep local volume changes out of sync ([c6c2556](https://github.com/heroesofcode/woodpeck/commit/c6c25569416c878438272f29d94d6d863b1055f1))
* **stats:** measure cycle accuracy over answered puzzles ([4d6d3fa](https://github.com/heroesofcode/woodpeck/commit/4d6d3faedbc50d1d4edebd9051d976e1ca8db8b5))


### Performance

* keep the training screen off blocking database work ([1c5c74a](https://github.com/heroesofcode/woodpeck/commit/1c5c74a2c81aeabda58befcda3caca81b439b8ae))

## [0.2.0](https://github.com/heroesofcode/woodpeck/compare/v0.1.0...v0.2.0) (2026-10-25)


### Features

* add application service and cancellable source jobs ([616550c](https://github.com/heroesofcode/woodpeck/commit/616550c3f8a2bbff83a5a4e7fd7c126af3936071))
* add local training rewards and optional completion chimes ([0ea8b3d](https://github.com/heroesofcode/woodpeck/commit/0ea8b3d78a3fb11a7b22bd5fcaaa9e46dd1e674c))
* add presets and stratified set generation ([3b2f4f6](https://github.com/heroesofcode/woodpeck/commit/3b2f4f67c381e57f78b843ff2bf6e26ed22804f0))
* add puzzle engine and solution walkthrough ([726c535](https://github.com/heroesofcode/woodpeck/commit/726c5353b2ba30db20f00c09cb37f990c03015bb))
* add puzzle library onboarding and measured streaming presets ([660dfae](https://github.com/heroesofcode/woodpeck/commit/660dfae7892baa88ada815f25d80e15d5e8c40f1))
* add puzzle source parsers and pgn export ([14b9bba](https://github.com/heroesofcode/woodpeck/commit/14b9bbad2db39d1345619d97f7f3ee5b48071335))
* add review queue domain rules ([633cf2c](https://github.com/heroesofcode/woodpeck/commit/633cf2cc83a0c78dd691ca7776bdf1de64c565a5))
* add streaming puzzle imports and lichess client ([bd9cb11](https://github.com/heroesofcode/woodpeck/commit/bd9cb11a4c1866be6ed29b42f02c67906672bee2))
* add timer layout settings and cycle statistics ([94b91bd](https://github.com/heroesofcode/woodpeck/commit/94b91bdf9dd9c1f205ad242a8beacb2572987530))
* **cli:** add sync initialization and status commands ([65beb91](https://github.com/heroesofcode/woodpeck/commit/65beb91c52b620018549e6955fab927e3685c92e))
* expose puzzle imports through a minimal cli ([200bb38](https://github.com/heroesofcode/woodpeck/commit/200bb38dd17549bcc940bacdf486d02fbb56aeb8))
* persist sets cycles attempts and review atomically ([87f3c76](https://github.com/heroesofcode/woodpeck/commit/87f3c7617b9b2e5c7b45bb8107a6c7712227837f))
* resolve omarchy palettes and board themes ([e7d5c80](https://github.com/heroesofcode/woodpeck/commit/e7d5c80dc4b454d835422f8ce0490a1ec3852ef9))
* **service:** map rating ranges to Lichess API difficulties ([012b6c3](https://github.com/heroesofcode/woodpeck/commit/012b6c3de6dd97f97151ba42465ab94ea064a44b))
* **service:** rotate included themes in the Lichess fallback ([51efe88](https://github.com/heroesofcode/woodpeck/commit/51efe88710a81905789a5f21cd6f03e53ac7e28a))
* **sets:** add include and exclude theme filters ([f0a087a](https://github.com/heroesofcode/woodpeck/commit/f0a087a7e07c387628cf419d2bef4903fbcec83d))
* **sets:** replace target rating and variance with an inclusive rating range ([a95c439](https://github.com/heroesofcode/woodpeck/commit/a95c43970acf86e8b9a0fb27679c76f619224ec3))
* **store:** add stable uids for synced records ([2f2a4e3](https://github.com/heroesofcode/woodpeck/commit/2f2a4e340451cf1cd2c226c6503040f688ee3e9f))
* **sync:** add bounded git transport and background worker ([632c260](https://github.com/heroesofcode/woodpeck/commit/632c260db12fecaeee24262280ca6c7e01a94f6d))
* **sync:** add diff-friendly sync format ([38346f7](https://github.com/heroesofcode/woodpeck/commit/38346f7cdb400b3021529721b2569297f0abdb4c))
* **sync:** bump portable format to schema 2 for theme filters ([ff7cf42](https://github.com/heroesofcode/woodpeck/commit/ff7cf42a1d2ddaac90c101165e8d9e313f03c221))
* **sync:** bump portable format to schema 3 with rating ranges ([c30842c](https://github.com/heroesofcode/woodpeck/commit/c30842c865c264900240b01a4fb773aa7d50802f))
* **sync:** merge portable progress by stable identity ([6c0c7ac](https://github.com/heroesofcode/woodpeck/commit/6c0c7acf2cdc7210dfcbdff5eb0d41bb4a1cc8f1))
* **ui:** add dashboard set builder progress and settings screens ([a799cc0](https://github.com/heroesofcode/woodpeck/commit/a799cc01bd7727ce25ac508adb84ee80fa7ac1c1))
* **ui:** add sync settings and themed status indicator ([cb35ffc](https://github.com/heroesofcode/woodpeck/commit/cb35ffcbd586ad44a8b66e3081c542d60c5f4178))
* **ui:** add themed chess board and presentation primitives ([ab9dece](https://github.com/heroesofcode/woodpeck/commit/ab9deced7c579151ed5bf3777c71d691dc00359a))
* **ui:** add training walkthrough and review queue screens ([4e84123](https://github.com/heroesofcode/woodpeck/commit/4e8412357a0f1d249cf0d0b4d212aa07d0374338))
* **ui:** dual-handle rating range slider in the set builder ([d537e55](https://github.com/heroesofcode/woodpeck/commit/d537e553e5f7a64bbda42372fddf19fbc83c2406))
* **ui:** ease library counters and clarify starter import progress ([93f7589](https://github.com/heroesofcode/woodpeck/commit/93f7589db71d5a2259f94ce288e4d58f8c8de718))
* **ui:** fill the live rating chart toward the expected library profile ([bc64559](https://github.com/heroesofcode/woodpeck/commit/bc645597de149d09673f667d37acfa1b15286f4a))
* **ui:** group digits in puzzle library counts ([3ae4a7b](https://github.com/heroesofcode/woodpeck/commit/3ae4a7b6e9bf7c577d73ea09f06e6f1d8601f3f0))
* **ui:** searchable include/exclude theme picker in the set builder ([852c2c9](https://github.com/heroesofcode/woodpeck/commit/852c2c969c855bfbda88a26e682ae3f097aae8e6))
* **ui:** show set theme filters on dashboard cards ([a3f7346](https://github.com/heroesofcode/woodpeck/commit/a3f7346fef9377bc2d4900f0491700110da8e5f6))
* **ui:** wire responsive desktop shell and screenshot self-check ([b2104cc](https://github.com/heroesofcode/woodpeck/commit/b2104cc303b4bdb835086b2de6aad4fdd7f52fb3))


### Bug fixes

* handle full dump zstd prefixes and propagate import errors ([1bef554](https://github.com/heroesofcode/woodpeck/commit/1bef5545bfbbfc37823f99e897932af8295e9b84))
* **library:** infer unmarked sizes and queue startup downloads ([f174ac0](https://github.com/heroesofcode/woodpeck/commit/f174ac05007861054350dd908c74376a227373be))
* prevent overflow in lifetime level progress ([23f50bd](https://github.com/heroesofcode/woodpeck/commit/23f50bdae1cbbe7728b25ea8cbef5f65ebf450bc))
* reject puzzle lines without a player move ([a757661](https://github.com/heroesofcode/woodpeck/commit/a757661b5f066f268a6c24950548f0fd404fcc63))
* **sets:** exclude category tags from the general diversity cap ([20b95c0](https://github.com/heroesofcode/woodpeck/commit/20b95c02f9bf6af4226c7e8b63e9cf514daada98))
* **sync:** preserve metadata clocks and deduplicate cycle display ([232c90e](https://github.com/heroesofcode/woodpeck/commit/232c90ec6c58467cb820e1c6f9686b78f67b210e))
* **sync:** preserve successful state when closing during sync ([f83aa4c](https://github.com/heroesofcode/woodpeck/commit/f83aa4c06bbbc9a2a9ab2c0b81fbd3bf9f759768))
* **sync:** preserve user settings across worker writes ([d65ef0c](https://github.com/heroesofcode/woodpeck/commit/d65ef0cae5924ffc3161dd6f95ee26d0761da04d))


### Refactoring

* split desktop views into focused components ([a07a53c](https://github.com/heroesofcode/woodpeck/commit/a07a53cb7e5c50edb37e549f29654bb55381020a))
