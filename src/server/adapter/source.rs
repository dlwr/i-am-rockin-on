use crate::domain::recommendation::NewRecommendation;
use crate::server::error::AppResult;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CandidateRef {
    pub source_external_id: String,
    pub source_url: String,
    /// 保存行の source_id 上書き。 None ならアダプタ既定（fetch_and_extract が決める）を使う。
    /// funkstudy が「どのハッシュタグでマッチしたか」を list_candidates → fetch_and_extract へ運ぶ。
    pub source_id_override: Option<String>,
}

#[async_trait]
pub trait MediaSource: Send + Sync {
    fn id(&self) -> &'static str;

    /// 一覧ページから候補記事 URL を列挙。
    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>>;

    /// 単記事を取得して、推しならば NewRecommendation の素材を返す。
    /// Spotify 解決前の段階（spotify_url 等は None）。
    async fn fetch_and_extract(&self, candidate: &CandidateRef) -> AppResult<Option<NewRecommendation>>;
}
