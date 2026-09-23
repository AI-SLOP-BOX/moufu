# Moufu (毛布)

<p align="center">
  <img src="./icon.webp" alt="Moufu Icon" width="160" height="160" />
</p>

<p align="center">
  アプリケーション間を動的リンクでつなぐ汎用 Integration Hub（開発中）
</p>

---

## 概要

Moufu（毛布）は、異なる制作アプリケーション間で編集データ・状態・依存関係を共有し、複数アプリを一つの制作環境のように連携させるためのハブです。

ファイル管理ではなく、アプリ同士の編集体験をつなぐ動的リンクを目的としています。

### 目標とする連携先
- Blender
- Kdenlive
- Hirari
- Amata
- Kagari
- Nagisa

---

## 現在の状況 (WIP / MVP)

現在は初期プロトタイプ段階です。Core/Protocol/Adapter SDK の分離設計と、Amata↔Kagari 間で未保存の編集通知を中継する最小構成の検証を進めています。

- **`crates/moufu-protocol`**: 通信メッセージ・データ型・ケイパビリティ定義
- **`crates/moufu-core`**: リンクグラフ管理・イベント中継・常駐エンジン
- **`crates/moufu-adapter-sdk`**: 各アプリ側へ組み込むためのクライアントSDK
- **`crates/moufu-gui`**: 接続状況やリンク状態を確認するコントロールセンター (egui)
- **`examples/`**: 動作検証用のモック

---

## ライセンス

MIT OR Apache-2.0
