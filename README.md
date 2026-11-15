# DiskGauge

ディスク使用量を可視化するデスクトップアプリです。ドライブやフォルダを並列スキャンし、容量を圧迫しているフォルダやファイルをツリーマップ／サンバーストで一目で把握できます。

Rust + [egui (eframe)](https://github.com/emilk/egui) 製で、Windows・macOS・Linux で動作します。

## コンセプト

DiskGauge は **DiskInfo** に影響を受けて作りました。DiskGaugeは、「起動すればすぐにディスクの状態がわかる」「何が容量を使っているのか」をひと目で見せることを目指しているマルチプラットフォーム対応のアプリです。

## 主な機能

- **ドライブ一覧** — 起動時に接続中のドライブ（マウントポイント）を使用率つきで表示。クリックするだけで解析を開始
- **任意フォルダの解析** — パスを直接入力するか「参照…」から選択
- **高速な並列スキャン** — [rayon](https://github.com/rayon-rs/rayon) によるマルチスレッド走査。進捗・残り時間の目安・処理速度をリアルタイム表示し、途中で中止して部分結果を見ることも可能
- **2 種類の可視化**
  - ツリーマップ
  - サンバースト（最大 7 階層）
  - 色分けは「拡張子」「フォルダ」から選択
- **フォルダツリー** — サイズ順に並んだ階層ツリーで内訳を確認
- **大きいファイル TOP300** — 表示中フォルダ配下の巨大ファイルを一覧
- **拡張子別集計** — 拡張子ごとの合計サイズと件数。クリックでその拡張子をビュー上で強調表示
- **フォルダ再解析** — 右クリックメニューの「フォルダ再解析」で、選択したフォルダ以下だけをスキャンし直して結果を更新（表示位置・選択・展開状態は維持）
- **ファイルマネージャー連携** — 右クリックメニューからフォルダを開く／ファイルの場所を表示／フルパスをコピー
- **ドライブ使用量の内訳** — 解析済み・未計上（システム領域やアクセス不可）・空き容量を比較表示

## 操作方法

| 操作 | 動作 |
| --- | --- |
| クリック | 項目を選択 |
| ダブルクリック | フォルダに入る |
| 右クリック | コンテキストメニュー |
| Backspace / 「↑ 上へ」 | 上の階層へ戻る |
| Esc | 拡張子の強調表示を解除 |
| サンバースト中央をクリック | 上の階層へ戻る |

## ビルドと実行

[Rust](https://www.rust-lang.org/tools/install)（edition 2024 対応のため 1.85 以降）が必要です。

```sh
# 開発ビルドで実行
cargo run

# リリースビルド
cargo build --release
```

リリースビルドの実行ファイルは `target/release/DiskGauge`（Windows では `DiskGauge.exe`）に出力されます。Windows のリリースビルドではコンソールウィンドウは表示されません。

### アプリアイコン

アイコンは `assets/icon/` にあり、OS ごとに次のように使われます。

| OS | 使われ方 | ファイル |
| --- | --- | --- |
| Windows | `build.rs` が `DiskGauge.exe` に埋め込み（エクスプローラー・ショートカット）。ウィンドウ／タスクバーは実行時に設定 | `icon.ico`（16〜256px）、`icon-256.png` |
| macOS | 実行時に Dock アイコンを設定。`.app` バンドルは `cargo bundle --release`（[cargo-bundle](https://github.com/burtonageo/cargo-bundle)）で作成 | `icon.icns`、`icon-macos-512.png` |
| Linux | 実行時にウィンドウアイコンを設定。デスクトップ環境のメニューに出すには下記のとおりインストール | `linux/` 以下 |

Windows で exe へのアイコン埋め込みには、Visual Studio Build Tools（Windows SDK）に含まれる `rc.exe` を使います。見つからない場合は警告を出してアイコンなしでビルドを続けます。

Linux でアプリメニューにアイコン付きで登録する例（ユーザー単位）:

```sh
cp target/release/DiskGauge ~/.local/bin/
cp -r assets/icon/linux/hicolor ~/.local/share/icons/
cp assets/icon/linux/diskgauge.desktop ~/.local/share/applications/
gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
```

元データは `assets/icon/icon.svg`（macOS 用は `icon-macos.svg`）です。

### Linux での追加要件

eframe のビルド・実行には X11 / Wayland 関連の開発パッケージが必要になる場合があります。また、日本語表示のために Noto Sans CJK や IPA フォントなどの日本語フォントをインストールしてください（見つからない場合は `fc-match` で検索します）。

## プラットフォームごとの挙動

| | Windows | macOS | Linux |
| --- | --- | --- | --- |
| ドライブ取得 | Win32 API (`GetLogicalDrives` など) | sysinfo | sysinfo（擬似ファイルシステムは除外） |
| 日本語フォント | メイリオ / 游ゴシック / MS ゴシック | ヒラギノ角ゴシック ほか | Noto Sans CJK / IPA ほか |
| 場所を表示 | エクスプローラーで選択 | Finder で表示 | FileManager1 D-Bus（不可ならフォルダを開く） |

### スキャン仕様の注意点

- シンボリックリンクやジャンクションは辿りません（二重計上やループ防止のため）
- macOS / Linux では開始地点と同じファイルシステム内のみを走査します（`du -x` と同様）
- macOS では起動ディスクのデータボリューム（`/System/Volumes/Data`）を解析対象として表示します
- Windows では OneDrive の「オンラインのみ」などクラウドのプレースホルダーファイルはサイズ 0 として扱います
- 権限不足で読めなかったフォルダは「アクセス不可」として件数を表示します。減らすには以下の方法があります
  - Windows: 管理者として実行
  - macOS: システム設定 > プライバシーとセキュリティ > フルディスクアクセス で許可
  - Linux: `sudo` で実行

## ソース構成

| ファイル | 役割 |
| --- | --- |
| `src/main.rs` | エントリーポイント・ウィンドウ設定（アイコン含む） |
| `build.rs` | Windows 実行ファイルへのアイコン埋め込み |
| `assets/icon/` | アプリアイコン（SVG / ICO / ICNS / PNG、Linux 用 .desktop） |
| `src/app.rs` | UI 全体（上部バー、ツリー、右パネル、可視化、ステータスバー） |
| `src/scan.rs` | 並列ディレクトリスキャンと進捗管理 |
| `src/tree.rs` | スキャン結果のフラットなツリー構造と集計 |
| `src/treemap.rs` | ツリーマップのレイアウトと描画 |
| `src/sunburst.rs` | サンバーストのレイアウトと描画 |
| `src/drives.rs` | ドライブ／マウントポイントの列挙 |
| `src/platform.rs` | OS 固有処理（フォント、ファイルマネージャー連携、文言） |
| `src/util.rs` | 配色・書式化などの共通処理 |

## ライセンス

[MIT License](LICENSE) で公開しています。
