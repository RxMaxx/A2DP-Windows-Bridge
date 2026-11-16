---
title: Збирання
layout: default
parent: Головна
nav_order: 3
---

# Збирання з вихідного коду
{: .no_toc }

## Зміст
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Вимоги {#requirements}

- **Rust** (stable, 1.90 або новіший) через [rustup](https://rustup.rs/)
- **Visual Studio 2022 або новіша** (або її Build Tools) з робочим навантаженням «Desktop development with C++»: бібліотеки C (BTstack, кодеки) компілюються за допомогою MSVC
- **Node.js** 22 і **pnpm** для інтерфейсу (`corepack enable` надає pnpm)
- **Git** (для підмодулів)
- **PowerShell 7** (`pwsh`) для `build.ps1` / `build.bat` (`winget install Microsoft.PowerShell`)
- **Python 3.11 або новіший** для перевірок (`tools/check_lang.py`) і [тесту з емулятором](#emu)
- Для збірок ARM64: компоненти Visual Studio «MSVC v143 - VS 2022 C++ ARM64 build tools» і, на ПК x64, «C++ Clang Compiler for Windows» (`ring` із rustls асемблює свій код ARM64 за допомогою clang; `installer/build-release.ps1` сам знаходить clang із Visual Studio), а також `rustup target add aarch64-pc-windows-msvc`

## Швидке збирання {#quick-build}

```bash
git clone --recursive https://github.com/SeiyaFunaokaJP/BlueCaster.git
cd BlueCaster
pwsh ./build.ps1
```

Або двічі клацніть `build.bat` у Провіднику: він запускає `build.ps1` і наприкінці чекає натискання клавіші (`build.bat -Test` передає ключі далі). `build.ps1` спершу перевіряє наявність Rust, Node.js, pnpm і підмодулів, запускає `pnpm install`, якщо немає `node_modules`, збирає сторінку вікна, а потім три програми в `target/release/`: `bluecaster-core.exe` (резидентне ядро: Bluetooth, звук, область сповіщень), `bluecaster.exe` (вікно) і `bluecaster-driver.exe` (допоміжна програма з правами адміністратора, яка призначає / відновлює драйвер WinUSB). Тримайте всі три в одній папці; `bluecaster-core.exe --cli` передає потік без вікна.

Ключі: `-Debug` (профіль dev, `target/debug/`), `-Test` (потім перевірки з розділу «Тести»), `-Decoder` (також інструмент декодування), `-Emu` (також [тест з емулятором](#emu); `-Emu -Codecs sbc,ldac` перевіряє лише ці кодеки), `-Release -Arch x64|arm64` (лише скрипт випуску, див. «Пакети випуску»).

**Покроково** (що робить `build.ps1`; сторінку вікна потрібно зібрати до `bluecaster.exe`, який її вбудовує):

```bash
cargo build --release -p bluecaster-core-exe -p bluecaster-driver
cd app/src/ui
pnpm install --frozen-lockfile
pnpm build
cd ../../..
cargo build --release -p bluecaster-ui
```

## Якщо ви вже клонували без `--recursive`

```bash
git submodule update --init --recursive
```

Виконайте цю команду знову після pull, який оновлює бібліотеку. Збирання зупиняється, якщо бібліотека в `extern/` не на коміті, указаному в `extern/versions.txt`; у повідомленні сказано, як усе ж зібрати з іншою версією.

## Структура проєкту {#project-structure}

```
BlueCaster/
├── app/
│   ├── src/
│   │   ├── core/           bluecaster-core.exe: resident core, tray, IPC server, --cli
│   │   ├── ui/             bluecaster.exe: Tauri 2 + SolidJS (pnpm dev runs it in a browser with a mock core)
│   │   ├── driver/         bluecaster-driver.exe: WinUSB assign / restore (elevated)
│   │   └── crates/         Libraries: btstack-sys, hci-transport, bt-host, codec, the codec -sys crates,
│   │                       pipeline, audio-capture, win-sys, core (bluecaster-core), ipc, le-audio, lc3-sys,
│   │                       aptx-adaptive (aptX Adaptive encoder)
│   ├── lang/               Localization (en.json is the source; tools/check_lang.py checks the others)
│   └── resources/          Icon and manifest
├── Cargo.toml              Cargo workspace
├── installer/              Release script, Inno Setup script, updater notes (installer/README.md)
├── compat/                 MSVC compatibility headers for the AOSP code
├── docs/                   Documentation (this site)
├── extern/                 Third-party C libraries (git submodules)
│   ├── btstack/            BTstack Bluetooth stack
│   ├── libldac/            AOSP LDAC encoder
│   ├── libopenaptx/        aptX / aptX HD / aptX LL encoder
│   ├── fdk-aac/            Fraunhofer AAC encoder
│   ├── lhdcv5-enc/         LHDC V5 encoder
│   └── opus/               libopus (Opus encoder)
└── tools/
    ├── a2dp_decode/        bluecaster_decode: checks / decodes the media stream in an HCI capture (.pklg)
    ├── emu/                End-to-end test against a virtual Bluetooth sink (Python)
    └── linux_sink/         bluecaster_sink: measuring A2DP receiver on Linux (Python, Bumble)
```

## Тести {#tests}

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd app/src/ui && pnpm exec tsc --noEmit && pnpm exec vitest run
python tools/check_lang.py
```

## Пакети випуску {#release-packages}

```powershell
pwsh installer/build-release.ps1 -Arch x64
```

записує в `target/dist` інсталятор (`BlueCaster-<version>-win64-setup.exe`), портативний ZIP-архів, ZIP-архів символів (файли `.pdb`, для аварійних дампів) і маніфест оновлювача. Потрібен Inno Setup 7.1.0 (`winget install JRSoftware.InnoSetup.7 --version 7.1.0`). Підписування оновлень у програмі та інші подробиці описано в `installer/README.md`.

## Тестування без обладнання (tools/emu) {#emu}

`tools/emu` передає кожен кодек від BlueCaster на віртуальний Bluetooth-приймач на тому самому ПК, без адаптера й навушників. Він працює лише у Windows і потребує **Python 3.11 або новішого** та **CMake** (входить до Visual Studio) для інструмента декодування.

```bash
python tools/emu/setup_env.py                       # once: creates tools/emu/.venv with pinned packages
cmake -S tools/a2dp_decode -B build/a2dp_decode     # once: the decoder tool
cmake --build build/a2dp_decode --config Release
cargo build --release -p bluecaster-core-exe
python tools/emu/run_test.py                        # every codec and negotiation case
```

Як це працює:

- `emu_sink.py` запускає два віртуальні контролери [Bumble](https://github.com/google/bumble), з'єднані всередині процесу. Один надається BlueCaster як H4 через TCP (`127.0.0.1`). Інший використовує A2DP-приймач Bumble, який пропонує SBC (з підтримкою SBC XQ), AAC, aptX, aptX HD, aptX LL, LDAC, LHDC V5, FastStream, Opus, Opus 05 і aptX Adaptive та записує свій трафік HCI.
- `run_test.py` запускає `bluecaster-core.exe --cli` для кожного кодека з параметрами для розробки: `--hci-tcp` (віртуальний контролер замість WinUSB), `--test-tone` (1 kHz ліворуч / 1,5 kHz праворуч замість системного звуку), `--duration`, `--hci-capture`. Змінна середовища `BLUECASTER_CONFIG_DIR` вказує BlueCaster окрему папку конфігурації, тож сполучення з віртуальним приймачем ніколи не зачіпає ваші справжні ключі зв'язку.
- Потім обидва записи HCI перевіряються за допомогою `bluecaster_decode` (бік приймача — з `--received`).

Кодек проходить тест, якщо виконуються всі умови:

- узгоджено саме запитаний кодек
- `bluecaster_decode` не знаходить проблем ні на боці надсилання, ні на боці приймання
- приймач отримав кожен медіапакет і кожен кадр, надісланий BlueCaster, і надіслано щонайменше 90 % запитаної тривалості звуку
- крім LDAC, LHDC V5 і aptX Adaptive (немає декодера з відкритим кодом): декодований звук однаковий на обох боках і є тестовим тоном

| Параметр | Опис |
|:-------|:------------|
| `--codecs sbc aac ...` | Перевірити лише ці кодеки (`sbc sbcxq aac aptx aptxhd aptxll ldac lhdcv5 faststream opus opus05`) або випадки узгодження (частоти дискретизації, низькі bitpool, конфігурації кадрів SBC, aptX Adaptive, ...; `-h` виводить їх список); за замовчуванням: усі |
| `--le-audio [case ...]` | Натомість виконати випадки LE Audio (за замовчуванням: усі) |
| `--scan [case ...]` | Натомість виконати випадки пошуку пристроїв (`eir`, `name-request`) |
| `--link [case ...]` | Натомість виконати випадки з'єднання (`disconnect`, `device-link`, `device-a2dp`, `device-stream`, `restart-on-link`) |
| `--duration <s>` | Секунд передавання на кодек (за замовчуванням 5) |
| `--max-packet <bytes>` | Передається BlueCaster (максимальний розмір медіапакета, див. [Використання](usage)) |
| `--bluecaster <exe>` | Ядро для перевірки (за замовчуванням `target/release/bluecaster-core.exe`) |
| `--decoder <exe>` | Інструмент декодування (за замовчуванням `<build-dir>/a2dp_decode/<config>/bluecaster_decode.exe`) |
| `--build-dir <dir>`, `--config <cfg>` | Де зібрано інструмент декодування (за замовчуванням `build`, `Release`) |
| `--base-port <port>` | Перший порт TCP (за замовчуванням 9101; випадки LE Audio використовують +100); змініть його, щоб запускати кілька тестів одночасно |
| `--out <dir>` | Вихідна папка (за замовчуванням `tools/emu/out`) |

Для кожного кодека `tools/emu/out/<codec>/` містить записи (`bluecaster.pklg`, `sink.pklg`), журнали, звіти `bluecaster_decode` і декодовані файли WAV. Код виходу дорівнює 0, якщо всі кодеки пройшли тест.

{: .note }
Віртуальне з'єднання не має радіоканалу: воно перевіряє кодування, пакетизацію, сигналізацію AVDTP / L2CAP і сумісність із незалежним стеком Bluetooth (Bumble), але не умови радіоканалу, таймінги на справжньому обладнанні чи те, як справжні навушники відтворюють звук. Пакети Bumble — лише засоби тестування й не входять до BlueCaster (див. THIRD_PARTY_LICENSES.md).

`emu_vhci.py` робить те саме для стеку Bluetooth Linux: у Linux він під'єднує через `/dev/vhci` віртуальний адаптер до BlueZ, який вимірювальний приймач `tools/linux_sink` потім бере як справжній адаптер — його можна тестувати без обладнання, за потреби із втраченими або затриманими медіапакетами (`--drop`, `--stall`). Див. `tools/linux_sink/README.md`.

## Залежності {#dependencies}

Бібліотеки C є підмодулями git; Rust-крейти й npm-пакети інтерфейсу завантажують Cargo і pnpm (версії зафіксовано в `Cargo.lock` і `app/src/ui/pnpm-lock.yaml`; `cargo deny check` перевіряє їхні ліцензії).

| Бібліотека | Спосіб | Зафіксована версія | Ліцензія |
|:--------|:-------|:---------------|:--------|
| BTstack | підмодуль git | v1.8.3 | BTstack License (у стилі BSD-3-Clause з умовою щодо некомерційного використання) |
| Кодек SBC з Bluedroid | кодувальник перенесено на Rust; декодер (тести, інструменти) — з копії в BTstack | як у BTstack | Apache-2.0 |
| rijndael | входить до BTstack (`3rd-party/rijndael`) | як у BTstack | Суспільне надбання |
| liblc3 (lc3-google) | входить до BTstack (`3rd-party/lc3-google`) | як у BTstack | Apache-2.0 |
| micro-ecc | входить до BTstack (`3rd-party/micro-ecc`) | як у BTstack | BSD-2-Clause |
| libldac (AOSP) | підмодуль git | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 |
| libopenaptx | підмодуль git | 0.2.0 (не оновлювати) | LGPL-2.1+ |
| fdk-aac | підмодуль git | v3.0.0 | FDK AAC License |
| Кодувальник LHDC V5 | підмодуль git | як зафіксовано | Apache-2.0 |
| libopus | підмодуль git | v1.6.1 | BSD-3-Clause |
| Tauri, SolidJS, Rust-крейти | Cargo / pnpm | `Cargo.lock`, `pnpm-lock.yaml` | MIT / Apache-2.0 та інші (`licenses/third-party/rust-crates.txt` у випуску) |

{: .warning }
Оскільки BTstack скомпоновано разом із програмою, будь-який зібраний вами BlueCaster можна використовувати й поширювати лише для особистих некомерційних цілей, хоча сам вихідний код BlueCaster поширюється за подвійною ліцензією Apache-2.0 або MIT.

Повні відомості про ліцензії див. у [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/BlueCaster/blob/main/THIRD_PARTY_LICENSES.md).
