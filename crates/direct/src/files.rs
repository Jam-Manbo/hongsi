use std::time::Instant;

use crate::{Direct, Reply, AUTOLOGIN_GAP, CN2, MB};

impl Direct {
    pub async fn attachment(&self, cmid: i64, index: usize) -> Result<(String, Vec<u8>), Reply> {
        let s = self.current().await?;
        let a = self.find_assignment(&s, cmid).await?;
        let file = a
            .attachments
            .get(index)
            .cloned()
            .ok_or_else(|| Reply::error(404, "not_found", "파일을 찾지 못했어요."))?;
        let (_, bytes) = s.session.download(&file.url, 1024 * MB).await?;
        Ok((file.name, bytes))
    }

    pub async fn module_file(&self, cmid: i64, index: usize) -> Result<(String, Vec<u8>), Reply> {
        let s = self.current().await?;
        let m = s.session.module_contents(cmid).await?;
        let file = m
            .files
            .get(index)
            .cloned()
            .ok_or_else(|| Reply::error(404, "not_found", "파일을 찾지 못했어요."))?;
        let (_, bytes) = s.session.download(&file.url, 1024 * MB).await?;
        Ok((file.name, bytes))
    }

    pub async fn board_file(&self, cmid: i64, bwid: i64, index: usize) -> Result<(String, Vec<u8>), Reply> {
        let s = self.current().await?;
        let article = s.session.board_article(cmid, bwid).await?;
        let file = article
            .attachments
            .get(index)
            .cloned()
            .ok_or_else(|| Reply::error(404, "not_found", "파일을 찾지 못했어요."))?;
        let (_, bytes) = s.session.download(&file.url, 1024 * MB).await?;
        Ok((file.name, bytes))
    }

    pub async fn avatar(&self) -> Option<(String, Vec<u8>)> {
        let s = self.current().await.ok()?;
        s.session
            .avatar()
            .await
            .ok()
            .flatten()
            .filter(|(mime, _)| mime.starts_with("image/"))
    }

    pub async fn browser_url(&self, url: &str) -> String {
        if !url.starts_with(CN2) {
            return url.to_string();
        }
        let Ok(s) = self.current().await else { return url.to_string() };
        if s.autologin_at
            .lock()
            .ok()
            .and_then(|t| *t)
            .is_some_and(|t| t.elapsed() < AUTOLOGIN_GAP)
        {
            return url.to_string();
        }
        match s.session.autologin_url(url).await {
            Ok(link) => {
                if let Ok(mut t) = s.autologin_at.lock() {
                    *t = Some(Instant::now());
                }
                link
            }
            Err(e) => {
                tracing::warn!("클래스룸 자동 로그인 주소를 받지 못함: {e}");
                url.to_string()
            }
        }
    }
}
