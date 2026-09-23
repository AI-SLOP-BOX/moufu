# Moufu Integration Hub

<p align="center">
  <img src="./icon.webp" alt="Moufu Icon" width="180" height="180" />
</p>

<p align="center">
  <b>Adobe Dynamic Link を究極まで発展させた、汎用的なリアルタイム統合ハブ</b>
</p>

---

## 概要

Moufu は、単なるファイル管理や素材ブラウザーではありません。
異なるクリエイティブアプリケーション間で編集データ・状態・依存関係をリアルタイムに共有し、**複数のアプリケーションを一つの制作環境のように連携させる** ための Integration Hub です。

### 主な特徴
- **未保存の動的リンク (Live Link)**: 保存操作を行うことなく、ドラッグやスライダー等の未保存編集を即座に連携先アプリへプレビュー通知。
- **60fps イベント合流・流量制御 (Event Coalescing)**: 高周波の編集操作を自動集約し、受信側の遅延・パンクを防止。
- **共有メモリ (Shared Memory) ゼロコピー転送**: 4K動画フレームや高解像度ラスタライズ画像をCPU・帯域負荷なく共有可能。
- **リンクの永続化と自動復元 (Persistent Links & Re-binding)**: アプリ終了や再起動時にもリンク依存関係を自動検出・復元。
- **連携能力ネゴシエーション (Capability Negotiation)**: 接続アプリごとの能力（Live Link, Push Edits, 差分同期等）をハンドシェイク時に宣言・検出。
- **コントロールセンター GUI (egui)**: 接続中のアプリ、アクティブリンク、同期バージョン、リアルタイムアクティビティを監視。

---

## 構成モジュール

- `crates/moufu-protocol`: 通信プロトコル・メッセージスキーマ・データ型定義
- `crates/moufu-core`: 依存関係グラフ (`LinkGraph`)・セッション管理・イベント合流・TCPサーバー
- `crates/moufu-adapter-sdk`: 自作アプリ（Amata, Kagari, Hirari, Nagisa等）へ組み込むためのクライアントSDK
- `crates/moufu-gui`: `egui` によるコントロールセンターGUI
- `examples/mock-amata`: ベクターデザインツール（パブリッシャー）モック
- `examples/mock-kagari`: ビデオ編集・コンポジション（サブスクライバー）モック

---

## ライセンス

MIT OR Apache-2.0
