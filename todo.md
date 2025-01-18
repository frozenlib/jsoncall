# todo

## 高優先度

- 読み込みインターフェイスの効率化（入力のバッファ効率化(RawValue を利用)）
- テスト
  - 自実装同士の通信
  - 他実装との通信
- SmallString,SmallVec の利用
- RawBatch,Batch の詳細を隠す

## 通常優先度

- リクエストエラーと他のエラーから伝搬してきたエラーを区別できない
  - → ローカルのエラーは variant で表し、リモートのエラーは ErrorObject で表す
- エラーの詳細を隠せるようにする
- OutgoingBuffer がデータ追加を必要に応じて無視するようにする
- キャンセル対応
- notify のエラーをロギング
- notify の abort 待機
- 通知処理の graceful shutdown
- バッチ処理対応
- SSE に対応できるように書き込みインターフェイスを変更する

## 解決済み

- poll していない incomingRequest が消えないのはどうする？
  - poll は outgoingRequest に関係あり、incomingRequest には関係ない。outgoingRequest は消えなくても問題なさそう
