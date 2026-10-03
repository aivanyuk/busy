# Microsoft Store listing: Japanese (ja-JP)

The Japanese text for Partner Center's ja-JP listing, translated from [the English listing](../store-listing.md), which stays the source: change that first, then this. Limits and everything not listed here (product name, screenshots' files, properties, age ratings, pricing, submission options) are as there; the product name stays "busy — system monitor".

## Description

```
busy は、CPU、メモリ、GPU、ネットワーク、ディスク、バッテリー、温度の値をリアルタイムで Windows 11 のタスク バーに表示します。表示位置は通知領域の横またはタスク バーの左端です。値をクリックすると、詳細なフライアウトが開きます。

タスク バーの表示
各値はテキスト、グラフ、バー、または (ネットワークとディスクでは) 読み取り/書き込み速度として表示され、個別の色または負荷に応じた色で、ラベルの有無を選んで、それぞれの間隔で更新されます。ポインターを合わせると値が表示されます。収まらない値はタスク ボタンに重ねずに省かれます。タスク バーは横向きでも縦向きでもかまいません。

値ごとのフライアウト
値をクリックすると、過去数分間 (設定で 1 ～ 10 分) のグラフと詳細が表示されます。
CPU: システム、ユーザー、アイドルの時間、すべての論理プロセッサ、速度、温度、プロセス、スレッド、ハンドル、稼働時間。
メモリ: 使用中、変更済み、スタンバイ、空き、コミット済み、圧縮、ページ プールと非ページ プール。
GPU: 最も負荷の高いエンジン、専用メモリと共有メモリ、温度、電力、ファン、クロック、ドライバーと DirectX のバージョン。
ディスク: ボリュームと空き領域、応答時間、温度、読み取りと書き込みのバイト数。
ネットワーク: インターフェイス、Wi-Fi の周波数帯と信号強度、アドレス、送受信の合計。
バッテリー: 残り時間、消費電力、健全性、サイクル数。
センサー: すべての温度、さらにセンサー ツールが提供する場合はファン、電力、電圧。
CPU、メモリ、GPU、ディスクでは最も負荷の高いプロセスを一覧表示します。どのフライアウトからも、タスク マネージャーとその値の設定を開けます。

設定
Windows 11 スタイルの設定画面で、検索、キーボード操作、スクリーン リーダーに対応し、各値のライブ プレビューを表示します。ライト テーマとダーク テーマは Windows に合わせることも、手動で選ぶこともできます。変更はすぐに反映されます。タスク バーの値を右クリックすると、設定、タスク バーに表示、位置、終了を選べます。初回起動時の短いセットアップで、表示する値とタスク バーのどちら側に置くかを選べます。

温度
GPU の温度は、NVIDIA と AMD のグラフィックス、および WDDM 2.5 以降のドライバーでそのまま表示されます。CPU の温度、ファン、CPU 電力は、LibreHardwareMonitor または HWiNFO を実行し、「設定」の「詳細設定」で「サードパーティのセンサー ツール」をオンにすると表示されます (既定ではオフ)。

プライバシーと軽さ
busy はデータを一切収集せず、Microsoft Store からインストールした場合はネットワークへの要求も行いません。管理者権限もドライバーも不要で、画面がロックまたはオフの間は一時停止します。ネイティブの Win32 と Direct2D で作られた、小さな実行ファイル 1 つだけのアプリです。

busy は MIT ライセンスのオープン ソースです。

Windows 11 23H2 (ビルド 22631) 以降、x64 が必要です。
```

## Product features

One per line in Partner Center, without bullets:

```
Windows 11 のタスク バーに CPU、メモリ、GPU、ネットワーク、ディスク、バッテリー、温度の値を表示
各値をテキスト、グラフ、バー、読み取り/書き込み速度で表示し、個別の色または負荷に応じた色で表示
値ごとのフライアウトに、過去 1 ～ 10 分のグラフと詳細を表示
コアごとの CPU 負荷、メモリの内訳、GPU エンジンと VRAM、ディスクの応答時間、Wi-Fi の信号強度、バッテリーの健全性
CPU、メモリ、GPU、ディスクで最も負荷の高いプロセスを表示し、タスク マネージャーへのリンクも用意
NVIDIA、AMD、WDDM 2.5 以降のドライバーで GPU の温度を表示。CPU の温度は LibreHardwareMonitor または HWiNFO から取得 (オプトイン)
検索、キーボード操作、スクリーン リーダー、ライブ プレビューに対応した Windows 11 スタイルの設定
Windows に合わせることも手動で選ぶこともできるライト テーマとダーク テーマ
横向きと縦向きのタスク バーに対応し、通知領域の横または左端に表示
アカウント不要、テレメトリなし、ネットワーク アクセスなし、管理者権限不要
```

## Short description

```
CPU、メモリ、GPU、ネットワーク、ディスク、バッテリー、温度の値をリアルタイムで Windows 11 のタスク バーに表示し、クリックすると値ごとに詳細なフライアウトが開きます。アカウント不要、テレメトリなし、ネットワーク アクセスなし。
```

## Screenshot captions

| File | Caption |
|---|---|
| `docs/images/taskbar.png` | タスク バーの表示: CPU はグラフ、メモリはバー、GPU はテキスト、ネットワークとディスクは転送速度 |
| `docs/images/flyout-cpu.png` | CPU のフライアウト: 使用率のグラフ、システム / ユーザー / アイドル、論理プロセッサ、速度、プロセス |
| `docs/images/flyout-memory.png` | メモリのフライアウト: 使用量のグラフ、使用中 / 変更済み / スタンバイ / 空き、コミット済み、圧縮、プール |
| `docs/images/settings-dark.png` | ダーク テーマの設定画面と、各値のライブ プレビュー |
| `docs/images/settings-light.png` | ライト テーマの設定画面: Windows の起動時に開始、位置、更新間隔、履歴、テーマ |
| `docs/images/setup.png` | 初回起動時のセットアップ: 表示する値とタスク バーの位置を選択 |

## Search terms

```
システムモニター
タスクバー
CPU使用率
メモリ使用率
GPUモニター
ネットワーク速度
温度
```

## Copyright and trademark info

```
Copyright (c) 2026 busy contributors. MIT ライセンスのオープン ソースです。
```
