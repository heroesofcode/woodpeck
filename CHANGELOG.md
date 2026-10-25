# Changelog

## [0.3.0](https://github.com/heroesofcode/woodpeck/compare/v0.2.0...v0.3.0) (2026-10-25)


### Features

* responsive training layout and sound effects ([2f6803c](https://github.com/heroesofcode/woodpeck/commit/2f6803c8f4c7449e09560b96a6ed443f793c9685))


### Bug fixes

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
