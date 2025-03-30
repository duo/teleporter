use std::{ops::Bound, path::Path, sync::Arc, time::Duration, vec};

use anyhow::Result;
use grammers_client::types::Message;
use tantivy::{
    DateOptions, DateTime, Index, IndexReader, Order, SnippetGenerator, TantivyDocument, Term,
    collector::TopDocs,
    directory::MmapDirectory,
    doc,
    query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery},
    schema::{
        FAST, INDEXED, IndexRecordOption, STORED, Schema, TextFieldIndexing, TextOptions, Value,
    },
    tokenizer::{LowerCaser, Stemmer, TextAnalyzer},
};
use tokio::sync::{mpsc, oneshot};

use super::telegram_helper as tg_helper;

// 通道的缓冲区大小
const BUFFER_SIZE: usize = 1024;
// 提交的次数频率
const COMMIT_RATE: usize = 100;
// 提交的时间频率
const COMMIT_TIME: Duration = Duration::from_secs(30);
// 最长的片段长度
const SNIPPET_MAX_CHARS: usize = 50;

#[derive(Clone)]
pub struct IndexService {
    schema: Schema,
    reader: Arc<IndexReader>,
    query_parser: QueryParser,
    doc_sender: mpsc::Sender<TantivyDocument>,
    commit_sender: mpsc::Sender<oneshot::Sender<()>>,
}

impl IndexService {
    pub async fn new() -> Result<Self> {
        // 定义索引的Schema
        let mut schema_builder = Schema::builder();
        schema_builder.add_i64_field("chat_id", FAST | INDEXED);
        schema_builder.add_i64_field("message_id", FAST | INDEXED | STORED);
        schema_builder.add_i64_field("reply_to", FAST | INDEXED);
        schema_builder.add_date_field(
            "timestamp",
            DateOptions::from(INDEXED)
                .set_stored()
                .set_fast()
                .set_precision(tantivy::DateTimePrecision::Seconds),
        );
        let content_field = schema_builder.add_text_field(
            "content",
            TextOptions::default()
                .set_indexing_options(
                    TextFieldIndexing::default()
                        .set_tokenizer("jieba")
                        .set_index_option(IndexRecordOption::WithFreqsAndPositions),
                )
                .set_stored(),
        );
        let schema = schema_builder.build();

        // 确保目录存在
        let index_path = Path::new("tantivy");
        if !index_path.exists() {
            std::fs::create_dir_all(index_path)?;
        }

        let dir = MmapDirectory::open(index_path)?;
        let index = Index::open_or_create(dir, schema.clone())?;

        // 注册分词器
        let tokenizer = tantivy_jieba::JiebaTokenizer {};
        let analyzer = TextAnalyzer::builder(tokenizer)
            .filter(LowerCaser)
            .filter(Stemmer::default())
            .build();
        index.tokenizers().register("jieba", analyzer);

        let query_parser: QueryParser = QueryParser::for_index(&index, vec![content_field]);

        let mut index_writer = index.writer(50_000_000)?;

        let (doc_sender, mut doc_receiver) = mpsc::channel(BUFFER_SIZE);
        let (commit_sender, mut commit_receiver) =
            mpsc::channel::<oneshot::Sender<()>>(BUFFER_SIZE);

        // 启动索引写入线程
        tokio::spawn(async move {
            let mut added_docs = 0;
            let mut commit_timestamp = std::time::Instant::now();

            loop {
                tokio::select! {
                    Some(doc) = doc_receiver.recv() =>{
                        match index_writer.add_document(doc) {
                            Ok(_) => {
                                added_docs += 1;
                            }
                            Err(e) => {
                                tracing::error!("Failed to add document to index: {}", e);
                            }
                        }

                        // 满足阈值就提交
                        if (added_docs > COMMIT_RATE) || (commit_timestamp.elapsed() > COMMIT_TIME) {
                            if let Err(e) = index_writer.commit() {
                                tracing::warn!("Failed to commit index: {}", e);
                            }
                            added_docs = 0;
                            commit_timestamp = std::time::Instant::now();
                        }
                    }
                    Some(sender) = commit_receiver.recv() => {
                        if let Err(e) = index_writer.commit() {
                            tracing::warn!("Failed to commit index: {}", e);
                        } else {
                            tracing::info!("Index committed before shutdown");
                        }
                        let _ = sender.send(());
                        break;
                    }
                }
            }
        });

        Ok(Self {
            schema,
            reader: Arc::new(index.reader()?),
            query_parser,
            doc_sender,
            commit_sender,
        })
    }

    // 将Telegram消息添加到索引
    pub async fn index_message(&self, message: &Message) -> Result<()> {
        let document = doc!(
            self.schema.get_field("chat_id").unwrap() => message.chat().id(),
            self.schema.get_field("message_id").unwrap() => message.id() as i64,
            self.schema.get_field("reply_to").unwrap() => {
                tg_helper::get_topic_id(message).map_or(0, |v| v as i64)
            },
            self.schema.get_field("timestamp").unwrap() => {
                DateTime::from_timestamp_secs(message.raw.date as i64)
            },
            self.schema.get_field("content").unwrap() => message.text(),
        );

        Ok(self.doc_sender.send(document).await?)
    }

    // 搜索Telegram消息, 返回(消息ID, 时间戳, 片段)
    pub async fn search_messages(
        &self,
        chat_id: i64,
        reply_to: Option<i32>,
        keyword: &str,
        last_id: Option<i32>,
        page_size: u64,
    ) -> Result<Vec<(i32, i64, String)>> {
        let message_id_field = self.schema.get_field("message_id").unwrap();
        let timestamp_field = self.schema.get_field("timestamp").unwrap();

        let searcher = self.reader.searcher();

        // 添加chat_id的查询条件
        let mut occurs: Vec<(Occur, Box<dyn Query>)> = vec![(
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_i64(self.schema.get_field("chat_id").unwrap(), chat_id),
                IndexRecordOption::Basic,
            )),
        )];

        // 添加reply_to的查询条件(Topic消息)
        if let Some(reply_to) = reply_to {
            occurs.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_i64(
                        self.schema.get_field("reply_to").unwrap(),
                        reply_to as i64,
                    ),
                    IndexRecordOption::Basic,
                )),
            ));
        }

        // 添加last_id的查询条件
        if let Some(last_id) = last_id {
            occurs.push((
                Occur::Must,
                Box::new(RangeQuery::new_i64_bounds(
                    "message_id".to_string(),
                    Bound::Unbounded,
                    Bound::Excluded(last_id as i64),
                )),
            ));
        }

        // 添加关键词的查询条件
        if !keyword.trim().is_empty() {
            occurs.push((Occur::Must, self.query_parser.parse_query(keyword)?));
        }

        // 生成查询
        let query = BooleanQuery::new(occurs);

        // 查询并按message_id降序排序
        let top_docs: Vec<(i64, tantivy::DocAddress)> = searcher.search(
            &query,
            &TopDocs::with_limit(page_size as usize).order_by_fast_field("message_id", Order::Desc),
        )?;

        // 片段生成器
        let mut snippet_generator =
            SnippetGenerator::create(&searcher, &query, self.schema.get_field("content").unwrap())?;
        snippet_generator.set_max_num_chars(SNIPPET_MAX_CHARS);

        let mut result = Vec::new();
        for (_, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            let message_id = retrieved_doc
                .get_first(message_id_field)
                .unwrap()
                .as_i64()
                .unwrap();
            let timestamp = retrieved_doc
                .get_first(timestamp_field)
                .unwrap()
                .as_datetime()
                .unwrap();

            let snippet = snippet_generator.snippet_from_doc(&retrieved_doc);

            result.push((
                message_id as i32,
                timestamp.into_timestamp_secs(),
                snippet.to_html(),
            ));
        }

        Ok(result)
    }

    // 提交索引
    pub async fn commit(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.commit_sender.send(sender).await?;
        receiver.await?;
        Ok(())
    }
}
