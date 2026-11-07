# Changelog

## [0.9.0](https://github.com/heroesofcode/woodpeck/compare/v0.8.0...v0.9.0) (2026-11-07)


### Features

* **books:** bind side-to-move captions to diagrams by page geometry ([339dc23](https://github.com/heroesofcode/woodpeck/commit/339dc23823528b7ffba755aa13ef46c051e4c6ea))
* **books:** fill unknown sides from numbered solution keys ([11b8022](https://github.com/heroesofcode/woodpeck/commit/11b802211a4afa54a0749e79ecc4a5a378232143))
* **books:** fill unknown sides from scan-caption OCR ([899f2d6](https://github.com/heroesofcode/woodpeck/commit/899f2d69449a5a4e1c34d8a4b395571924ab4703))
* **books:** read side triangles anywhere and wider side captions ([988af14](https://github.com/heroesofcode/woodpeck/commit/988af14cf2bee460af1fbc174cc874bea5bb0ddc))
* **books:** search side-to-move marks on all sides of the board ([3ce9aed](https://github.com/heroesofcode/woodpeck/commit/3ce9aedcbc6f547f2ccd248708a2be76b96032bb))


### Bug fixes

* **books:** keep caption baselines within half a line ([c6f9f00](https://github.com/heroesofcode/woodpeck/commit/c6f9f00eeeb1c6b8558be8b387373923e2eeadeb))
* **books:** keep the scan-caption window on the located board ([ed0719a](https://github.com/heroesofcode/woodpeck/commit/ed0719a16fc0b0611dac420bec7dc5d4c2caa6d9))
* **books:** measure scan captions from the board, not its crop padding ([b4704ac](https://github.com/heroesofcode/woodpeck/commit/b4704ac3c63acfced7206fec61b911b1a263f409))
* **books:** read spaced dots in numbered solution keys ([5a589dd](https://github.com/heroesofcode/woodpeck/commit/5a589dd62293041e9e9a63284807473825b4e035))

## [0.8.0](https://github.com/heroesofcode/woodpeck/compare/v0.7.0...v0.8.0) (2026-11-06)


### Features

* **books:** add hatch-tuned classifier as third fallback ([a398dda](https://github.com/heroesofcode/woodpeck/commit/a398dda16bfdd99d215f028c2a0c4615e56d5444))


### Bug fixes

* **games:** align desktop controls and reclaim phone board space ([3d86863](https://github.com/heroesofcode/woodpeck/commit/3d86863c0984f6c918a83121938391ba8f8524b2))
* **games:** constrain picker cards to responsive rows ([a6711af](https://github.com/heroesofcode/woodpeck/commit/a6711af2da27c313d7e74bf6950423c945954dc7))
* **games:** explain safe moves and make blind line recall playable ([539b25f](https://github.com/heroesofcode/woodpeck/commit/539b25f3bb7cbe2d5d5047144599c8f594ff5c3c))
* **games:** fit drill boards and clarify board scan feedback ([77a7b0e](https://github.com/heroesofcode/woodpeck/commit/77a7b0e3309dddb99c30241585a6f566a8a81124))
* **games:** keep feedback inline and cover keyboard round loops ([e0e3b03](https://github.com/heroesofcode/woodpeck/commit/e0e3b03f058fb1dfac872a37bd2635f8f4eaf5a4))
* **games:** reset round feedback and verify memory replay ([513b918](https://github.com/heroesofcode/woodpeck/commit/513b9187fe5e33e5bfec9de946bb4d8192cddcb9))
* **games:** show human move choices and honest drill guidance ([c3d9d4e](https://github.com/heroesofcode/woodpeck/commit/c3d9d4e08abf396f3d18bdff2215a1a9028127c8))

## [0.7.0](https://github.com/heroesofcode/woodpeck/compare/v0.6.0...v0.7.0) (2026-11-05)


### Features

* add the browser entry point and web shell ([3295b11](https://github.com/heroesofcode/woodpeck/commit/3295b11e97b56b7ca676a984fd50c4d7b7b6e93b))
* **books:** confirm side to move from the square printed beside diagrams ([7b3b9cb](https://github.com/heroesofcode/woodpeck/commit/7b3b9cb70487853ce14ad70cbfe746a58196dea8))
* **books:** fall back to the previous diagram nets when the new ones abstain ([5fea6b1](https://github.com/heroesofcode/woodpeck/commit/5fea6b16bdb048ccc012030aafe6275d388b0255))
* **books:** fine-tune the diagram classifiers on newer book styles ([5c5fba7](https://github.com/heroesofcode/woodpeck/commit/5c5fba7a50410d81b10fd8d50862106dce81f47d))
* **books:** fine-tune the diagram classifiers on real book diagrams ([937b2d0](https://github.com/heroesofcode/woodpeck/commit/937b2d00ac759f76d9e65ddae26dd070380c9170))
* **books:** fine-tune the diagram classifiers on scanned and drawn pages ([6bd7589](https://github.com/heroesofcode/woodpeck/commit/6bd7589775c0c756ac3f40f11598b359285953f6))
* **books:** generate hatched printed-book diagrams for classifier training ([7abacd3](https://github.com/heroesofcode/woodpeck/commit/7abacd3e80ee6ee0b57efe6fac741071646391ed))
* **books:** locate a board by its border lines when it fills little of the image ([5a8be7a](https://github.com/heroesofcode/woodpeck/commit/5a8be7a18f8584f94762f1587978069220464043))
* **books:** read diagrams on scanned, drawn and font-set PDF pages ([d2ae09a](https://github.com/heroesofcode/woodpeck/commit/d2ae09a6ce66faf6d9fda53db96244198945aefe))
* **books:** resolve moves around diagram-font boards ([2649f41](https://github.com/heroesofcode/woodpeck/commit/2649f41a690e08f2dc7214cd989bec8a321b55fe))
* **books:** retrain the diagram classifiers on hatched book styles ([8633f0c](https://github.com/heroesofcode/woodpeck/commit/8633f0cbb574f03265a39044db87efab938d8118))
* **books:** show diagrams whose side to move the text does not confirm ([96ba3e8](https://github.com/heroesofcode/woodpeck/commit/96ba3e8dd3cba7fa6a4fae3c529074f878dcc90d))
* **games:** add Blind line visualization drill ([e822a51](https://github.com/heroesofcode/woodpeck/commit/e822a519933f950db4be86d22dec0ceb4c27d8d7))
* **games:** add Games hub and Board scan drill ([57d9e23](https://github.com/heroesofcode/woodpeck/commit/57d9e2308088e8e033baf1ef841b1314e4334fcb))
* **games:** add Human move drill ([efe1c4d](https://github.com/heroesofcode/woodpeck/commit/efe1c4da58def0b77127f169d6a22a1a1fc72376))
* **games:** add Safe move drill ([2d30c18](https://github.com/heroesofcode/woodpeck/commit/2d30c18704b4ec72a79e607473797b83bcdd825d))
* run the service job queue in the browser ([19161f9](https://github.com/heroesofcode/woodpeck/commit/19161f9c65f6955c8ed3d69dd4bdfa7e16815500))
* **sync:** add a GitHub API remote for the browser ([9eef502](https://github.com/heroesofcode/woodpeck/commit/9eef502333de1c9dadc6b3e3c5df43fe1ba80367))
* **sync:** pull other devices' changes every five minutes while open ([c61fb7a](https://github.com/heroesofcode/woodpeck/commit/c61fb7a7d4f37f4bcc013395e1e954b9da54c793))
* **web:** add GitHub repository and token sync settings ([985b86b](https://github.com/heroesofcode/woodpeck/commit/985b86bf58ea046a1b9ae07e300b28d9f0cd5314))
* **web:** offer only the starter pack in the browser ([36e7d66](https://github.com/heroesofcode/woodpeck/commit/36e7d66f23c683fa4f59d5c9a1b2bb4918c46eb6))
* **web:** sync the browser with GitHub on open, after changes and periodically ([9d42536](https://github.com/heroesofcode/woodpeck/commit/9d42536e777fcbf9291cc5c0baa6bec22661a5ba))


### Bug fixes

* **books:** abstain on an image that holds several boards ([2fcce17](https://github.com/heroesofcode/woodpeck/commit/2fcce1747bc77ba9eff95c29e5dbb560853af65d))
* **books:** cap oversized diagram crops before square nets ([a608df0](https://github.com/heroesofcode/woodpeck/commit/a608df0e278b71684a44b08a1e6484368029e533))
* **books:** confirm side from Polgar No. N xk/çk labels ([676a7dd](https://github.com/heroesofcode/woodpeck/commit/676a7dd7655b12b52a49479c6dd19942fb4aa829))
* **books:** confirm side from Quality Chess triangle markers ([266f156](https://github.com/heroesofcode/woodpeck/commit/266f156f44cd05f988088b17373623aba4217a90))
* **books:** confirm side to move from the last ply before a diagram ([b022c1e](https://github.com/heroesofcode/woodpeck/commit/b022c1e8a4dcbd9778c9a535587c9a7e0bd51e9b))
* **books:** decode percent-encoded EPUB image hrefs ([0c263da](https://github.com/heroesofcode/woodpeck/commit/0c263da647cebe309299b861051ab9957dd5f709))
* **books:** do not inherit mate themes past long commentary ([9ccd048](https://github.com/heroesofcode/woodpeck/commit/9ccd048349be76d1553f99bcf31b7ae19624cfb8))
* **books:** drop dead png stub from percent-href test ([82d71ab](https://github.com/heroesofcode/woodpeck/commit/82d71abd3b1d8de04f37fa1e020ae4c13877acbd))
* **books:** feed look-past captions into stated turn safely ([a26532c](https://github.com/heroesofcode/woodpeck/commit/a26532c081c6f3217b70967c8e3df16cb06b720a))
* **books:** keep diagram numbers and narrated moves from naming the side ([faae953](https://github.com/heroesofcode/woodpeck/commit/faae9534d75a826ac5a9204091b1161a5ba19659))
* **books:** keep figures beside font boards and skip rotated pages ([128250b](https://github.com/heroesofcode/woodpeck/commit/128250bb6dee09594f4b68c183c3f911a879cc31))
* **books:** keep importing a PDF when one page panics in the text extractor ([8fb088c](https://github.com/heroesofcode/woodpeck/commit/8fb088c4a426f7e7c24f948ea7aac8e8e9f27a4f))
* **books:** keep look-past captions off stated and prior boards ([76bcd6b](https://github.com/heroesofcode/woodpeck/commit/76bcd6bcc17056ee359304386981e73729157b33))
* **books:** keep trailing-ply side inference off marked boards ([319dbb4](https://github.com/heroesofcode/woodpeck/commit/319dbb4f4f7e718695305e7feb600d7ae2f228ea))
* **books:** leave the PDF cover unstamped when captioning sides ([e717e25](https://github.com/heroesofcode/woodpeck/commit/e717e255fad02dfa03c6ef2b8bb6fee3d1043ca1))
* **books:** let a PGN header start a new game root inside a fragment ([6b16240](https://github.com/heroesofcode/woodpeck/commit/6b162404a627b44e285562178c0b3a4e0ea70a0c))
* **books:** limit light-rim locate to embedded diagram assets ([80551e7](https://github.com/heroesofcode/woodpeck/commit/80551e79e0a5d2d792a04b114ccca9e0cdbb4156))
* **books:** locate boards inside thick light (gold) frames ([6e4a7ab](https://github.com/heroesofcode/woodpeck/commit/6e4a7abdcc7fcbefe79e226fd23d64c76a2c1808))
* **books:** look for the side marker beside the board, not its padded crop ([4b12f4f](https://github.com/heroesofcode/woodpeck/commit/4b12f4fe0842034596261ca7aa7664cfd1493fe1))
* **books:** name the page and move of an illegal emitted edge ([7219644](https://github.com/heroesofcode/woodpeck/commit/7219644bdf87e97cd54f2fbc3e8cfbdcfed6c576))
* **books:** never hang a root move from a position other than its fragment start ([198a4c9](https://github.com/heroesofcode/woodpeck/commit/198a4c9d75f71432a054d85eeae1d5b38ec58bd9))
* **books:** Prototypes reading tries capped cell grids too ([8e6f1a3](https://github.com/heroesofcode/woodpeck/commit/8e6f1a37af2a77df57d6ca986c72c1decd5d29e6))
* **books:** recover turn captions above bare diagram numbers ([34c73b8](https://github.com/heroesofcode/woodpeck/commit/34c73b893ef6e0dcfeab3ece3eb781377d78f96c))
* **books:** stamp side from pdftotext captions on textless PDF pages ([8217e21](https://github.com/heroesofcode/woodpeck/commit/8217e21ffaf1e0de0a8a37413167f8c0ee9fe316))
* **books:** try native then capped board cells for placement ([1b2e1fd](https://github.com/heroesofcode/woodpeck/commit/1b2e1fd117ab52a24773a1e39435d74178b173d2))
* **deps:** use ruzstd 0.9.1 for the browser decoder ([28ed7fe](https://github.com/heroesofcode/woodpeck/commit/28ed7fea81353503e60e688104f58228c06adbcc))
* **games:** avoid double-reserving board height ([9b65e64](https://github.com/heroesofcode/woodpeck/commit/9b65e647f0e827bc6554b7c05fd03c17e26b1e7e))
* **games:** flash red when a Safe move blunders ([fb9d5d5](https://github.com/heroesofcode/woodpeck/commit/fb9d5d501590b51947780e6e88a277a8127457f4))
* **games:** hub wrap, toast scope, blind answer, board fit ([c64bc13](https://github.com/heroesofcode/woodpeck/commit/c64bc13c793332ce117d2fa3bea7a51773b941f3))
* **games:** keep action row visible and center the board ([87139a2](https://github.com/heroesofcode/woodpeck/commit/87139a2a57f87bdd634e970cfe503c5254ad4a66))
* **games:** keep Blind line timed and orientation-stable ([51836ca](https://github.com/heroesofcode/woodpeck/commit/51836ca53fb1a029c2aa726d720cb980c9317c2e))
* **games:** keep Safe move board stable after a reply ([0ddf9b2](https://github.com/heroesofcode/woodpeck/commit/0ddf9b2d652925fdfce4d6a53b5dd0a1541a8673))
* **games:** show Games on web nav and keep icons stable ([67e5bb5](https://github.com/heroesofcode/woodpeck/commit/67e5bb5bdfe73a688774623e356aee242e178c2c))
* **web:** keep saving while a write is in flight ([32eddd6](https://github.com/heroesofcode/woodpeck/commit/32eddd6b85014b3c9c500eb591bc0a28ebd48e18))
* **web:** retry after bootstrap and sync immediately on open ([4813740](https://github.com/heroesofcode/woodpeck/commit/4813740f4798ea3b1f71908c56d5c61da32859ee))
* **web:** save only after IndexedDB commits and download PGN in the click ([cfa4f19](https://github.com/heroesofcode/woodpeck/commit/cfa4f192f54e373489945a152118b2f964de8c8c))


### Refactoring

* build the portable core for wasm32 ([f8d4de1](https://github.com/heroesofcode/woodpeck/commit/f8d4de132ec88fa793681390eb07a43402b8b3a1))
* extract background job steps for reuse ([77cdec8](https://github.com/heroesofcode/woodpeck/commit/77cdec89b43c3b012199a831889e51bde04522f4))
* share Lichess request planning across targets ([80beb00](https://github.com/heroesofcode/woodpeck/commit/80beb00fea4020a5bdd36ea42d96f7bf90a9a862))
* **sync:** put the Git transport behind a Remote trait ([6d58818](https://github.com/heroesofcode/woodpeck/commit/6d5881843a18ac3125abfc54f1dded351ec18320))
* use web-time for monotonic instants ([74634d8](https://github.com/heroesofcode/woodpeck/commit/74634d8e6be5f144da139da0203d051f842c5b06))

## [0.6.0](https://github.com/heroesofcode/woodpeck/compare/v0.5.0...v0.6.0) (2026-11-03)


### Features

* **reader:** bundle Source Serif for book typography ([4ab9690](https://github.com/heroesofcode/woodpeck/commit/4ab9690055b71ce45a53022ba75e0906ddeef186))
* **reader:** typeset book pages with cached interactive text ([08196bc](https://github.com/heroesofcode/woodpeck/commit/08196bc4b5425514ad98490476ec98a4512c7729))


### Bug fixes

* **books:** reconstruct PDF paragraphs from line geometry ([9cc787f](https://github.com/heroesofcode/woodpeck/commit/9cc787f0b47cd92e35142fe7f94809fe6cd4036d))
* **reader:** distinguish captions from ordinary prose ([e50d0fd](https://github.com/heroesofcode/woodpeck/commit/e50d0fdab0c16635d8a8779fda8f6c0b92a1fa02))
* **reader:** retain punctuated figure captions ([ec1aa31](https://github.com/heroesofcode/woodpeck/commit/ec1aa3198edd807d7b709063fde01ea8e21fc86e))


### Performance

* **reader:** benchmark idle frames and book navigation ([e1f24bd](https://github.com/heroesofcode/woodpeck/commit/e1f24bdb37efe8233f0c11ee940a1d4bda113c6a))

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
