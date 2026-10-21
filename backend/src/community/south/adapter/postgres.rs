use crate::business::{Actor, Decision, Error, Page, Result, require};
use crate::community::domain::{CommentInput, PostInput};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn validate_media(&self, s: Uuid, images: &[String]) -> Result<()> {
        for path in images {
            let id = path
                .rsplit('/')
                .next()
                .and_then(|x| x.parse::<Uuid>().ok())
                .ok_or_else(|| Error::Invalid("图片地址无效".into()))?;
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM media_assets WHERE school_id=$1 AND id=$2)",
            )
            .bind(s)
            .bind(id)
            .fetch_one(&self.0)
            .await?;
            require(exists, "图片不存在或属于其他学校")?;
        }
        Ok(())
    }

    pub async fn check_words(&self, s: Uuid, content: &str) -> Result<()> {
        let words:Vec<String>=sqlx::query_scalar("SELECT title FROM school_resources WHERE (school_id=$1 OR school_id IS NULL) AND kind='blocked_word' AND active").bind(s).fetch_all(&self.0).await?;
        let normalized = content.to_lowercase();
        require(
            !words.iter().any(|w| normalized.contains(&w.to_lowercase())),
            "内容包含禁止发布的词语",
        )
    }
    pub async fn posts(&self, a: &Actor, s: Uuid, p: Page) -> Result<Vec<Value>> {
        let (l, o) = p.fetch_bounds()?;
        let search = format!("%{}%", p.search.unwrap_or_default());
        Ok(sqlx::query_scalar("SELECT to_jsonb(p)||jsonb_build_object('author_name',a.display_name,'like_count',(SELECT count(*) FROM post_reactions WHERE post_id=p.id AND kind='like'),'collect_count',(SELECT count(*) FROM post_reactions WHERE post_id=p.id AND kind='collect'),'comment_count',(SELECT count(*) FROM comments WHERE post_id=p.id AND audit_status='approved' AND NOT deleted)) FROM posts p JOIN accounts a ON a.id=p.author_id WHERE p.school_id=$1 AND ($2 OR (p.audit_status='approved' AND p.visibility='visible') OR p.author_id=$3) AND ($4::text IS NULL OR p.audit_status=$4) AND (NOT $5 OR p.author_id=$3) AND (p.title ILIKE $6 OR p.content ILIKE $6) AND ($9::text IS NULL OR ($9='collect' AND EXISTS(SELECT 1 FROM post_reactions r WHERE r.post_id=p.id AND r.account_id=$3 AND r.kind='collect')) OR ($9='following' AND EXISTS(SELECT 1 FROM follows f WHERE f.school_id=p.school_id AND f.follower_id=$3 AND f.target_id=p.author_id))) ORDER BY p.top_sort DESC,p.created_at DESC,p.id LIMIT $7 OFFSET $8").bind(s).bind(a.can(s,"operator")).bind(a.id).bind(p.status).bind(p.mine.unwrap_or(false)).bind(search).bind(l).bind(o).bind(p.kind).fetch_all(&self.0).await?)
    }
    pub async fn detail(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<Value> {
        let v:Value=sqlx::query_scalar("UPDATE posts SET browse=browse+1 WHERE school_id=$1 AND id=$2 AND ($3 OR author_id=$4 OR (audit_status='approved' AND visibility='visible')) RETURNING to_jsonb(posts)||jsonb_build_object('author_name',(SELECT display_name FROM accounts WHERE id=posts.author_id),'like_count',(SELECT count(*) FROM post_reactions WHERE post_id=posts.id AND kind='like'),'comment_count',(SELECT count(*) FROM comments WHERE post_id=posts.id AND audit_status='approved' AND NOT deleted))").bind(s).bind(id).bind(a.can(s,"operator")).bind(a.id).fetch_optional(&self.0).await?.ok_or(Error::NotFound)?;
        Ok(v)
    }
    pub async fn create(&self, a: &Actor, s: Uuid, v: PostInput) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let mode: String = sqlx::query_scalar(
            "SELECT coalesce(config->>'post_audit_mode','manual') FROM schools WHERE id=$1",
        )
        .bind(s)
        .fetch_one(&mut *tx)
        .await?;
        let status = if mode == "auto" {
            "approved"
        } else {
            "pending"
        };
        let id = Uuid::now_v7();
        let item=sqlx::query_scalar("INSERT INTO posts(id,school_id,author_id,title,content,images,tags,audit_status) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING to_jsonb(posts)").bind(id).bind(s).bind(a.id).bind(v.title.trim()).bind(v.content.trim()).bind(json!(v.images)).bind(json!(v.tags)).bind(status).fetch_one(&mut *tx).await?;
        crate::audit::record(
            &mut tx,
            a,
            s,
            "post.create",
            id,
            json!({"audit_status":status}),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn update(&self, a: &Actor, s: Uuid, id: Uuid, v: PostInput) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let item=sqlx::query_scalar("UPDATE posts SET title=$3,content=$4,images=$5,tags=$6,audit_status='pending',revision=revision+1,updated_at=now() WHERE school_id=$1 AND id=$2 AND revision=$7 AND visibility<>'deleted' AND (author_id=$8 OR $9) RETURNING to_jsonb(posts)").bind(s).bind(id).bind(v.title.trim()).bind(v.content.trim()).bind(json!(v.images)).bind(json!(v.tags)).bind(v.revision.unwrap_or(0)).bind(a.id).bind(a.can(s,"operator")).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        crate::audit::record(&mut tx, a, s, "post.edit", id, json!({})).await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn visibility(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let item=sqlx::query_scalar("UPDATE posts SET visibility=$3,top_sort=$4,revision=revision+1,updated_at=now() WHERE school_id=$1 AND id=$2 AND revision=$5 AND ($6 OR (author_id=$7 AND $3='deleted' AND $4=0)) RETURNING to_jsonb(posts)").bind(s).bind(id).bind(v["visibility"].as_str()).bind(v["top_sort"].as_i64().unwrap_or(0) as i32).bind(v["revision"].as_i64().unwrap_or(0)).bind(a.can(s,"operator")).bind(a.id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        crate::audit::record(
            &mut tx,
            a,
            s,
            "post.visibility",
            id,
            json!({"visibility":v["visibility"]}),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn moderate(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Decision,
        comment: bool,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let sql = if comment {
            "UPDATE comments SET audit_status=$3,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$4 AND NOT deleted RETURNING to_jsonb(comments)"
        } else {
            "UPDATE posts SET audit_status=$3,revision=revision+1,updated_at=now() WHERE school_id=$1 AND id=$2 AND revision=$4 AND visibility<>'deleted' RETURNING to_jsonb(posts)"
        };
        let item: Value = sqlx::query_scalar(sql)
            .bind(s)
            .bind(id)
            .bind(&v.decision)
            .bind(v.revision)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Conflict)?;
        sqlx::query("INSERT INTO moderation_events VALUES($1,$2,$3,$4,$5,$6,$7,now())")
            .bind(Uuid::now_v7())
            .bind(s)
            .bind(a.id)
            .bind(id)
            .bind(if comment { "comment" } else { "post" })
            .bind(&v.decision)
            .bind(&v.reason)
            .execute(&mut *tx)
            .await?;
        let author = crate::business::uuid(&item, "author_id")?;
        sqlx::query("INSERT INTO notifications(id,school_id,account_id,kind,object_id) VALUES($1,$2,$3,'moderation',$4)").bind(Uuid::now_v7()).bind(s).bind(author).bind(id).execute(&mut *tx).await?;
        crate::audit::record(
            &mut tx,
            a,
            s,
            "moderation.decide",
            id,
            json!({"decision":v.decision,"reason":v.reason}),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn react(&self, a: &Actor, s: Uuid, id: Uuid, kind: &str, add: bool) -> Result<()> {
        let mut tx = self.0.begin().await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posts WHERE school_id=$1 AND id=$2 AND audit_status='approved' AND visibility='visible')").bind(s).bind(id).fetch_one(&mut *tx).await?;
        if !exists {
            return Err(Error::NotFound);
        }
        if add {
            sqlx::query("WITH changed AS (INSERT INTO post_reactions VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING RETURNING post_id) INSERT INTO notifications(id,school_id,account_id,kind,object_id) SELECT $5,$1,p.author_id,'like',p.id FROM posts p JOIN changed c ON c.post_id=p.id WHERE p.author_id<>$3 AND $4='like'")
                .bind(s)
                .bind(id)
                .bind(a.id)
                .bind(kind)
                .bind(Uuid::now_v7())
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("DELETE FROM post_reactions WHERE school_id=$1 AND post_id=$2 AND account_id=$3 AND kind=$4").bind(s).bind(id).bind(a.id).bind(kind).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn reactions(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('id',r.school_id::text||':'||r.account_id::text||':'||r.post_id::text||':'||r.kind) FROM post_reactions r JOIN posts p ON p.id=r.post_id WHERE r.school_id=$1 AND r.account_id=$2 AND p.audit_status='approved' AND p.visibility='visible'").bind(s).bind(a.id).fetch_all(&self.0).await?)
    }
    pub async fn comments(
        &self,
        a: &Actor,
        s: Uuid,
        id: Option<Uuid>,
        p: Page,
    ) -> Result<Vec<Value>> {
        let (l, o) = p.fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT to_jsonb(c)||jsonb_build_object('author_name',a.display_name) FROM comments c JOIN posts p ON p.id=c.post_id JOIN accounts a ON a.id=c.author_id WHERE c.school_id=$1 AND ($2::uuid IS NULL OR c.post_id=$2) AND ($3 OR (p.visibility='visible' AND p.audit_status='approved' AND NOT c.deleted AND (c.audit_status='approved' OR c.author_id=$4))) AND ($5::text IS NULL OR c.audit_status=$5) ORDER BY c.created_at DESC,c.id DESC LIMIT $6 OFFSET $7").bind(s).bind(id).bind(a.can(s,"operator")).bind(a.id).bind(p.status).bind(l).bind(o).fetch_all(&self.0).await?)
    }
    pub async fn comment(&self, a: &Actor, s: Uuid, id: Uuid, v: CommentInput) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let post=sqlx::query("SELECT p.author_id,coalesce(s.config->>'comment_audit_mode','manual') AS mode FROM posts p JOIN schools s ON s.id=p.school_id WHERE p.id=$1 AND p.school_id=$2 AND p.audit_status='approved' AND p.visibility='visible' FOR SHARE OF p").bind(id).bind(s).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        if let Some(parent) = v.parent_id {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM comments WHERE id=$1 AND post_id=$2 AND school_id=$3 AND audit_status='approved' AND NOT deleted)").bind(parent).bind(id).bind(s).fetch_one(&mut *tx).await?;
            if !valid {
                return Err(Error::NotFound);
            }
        }
        let cid = Uuid::now_v7();
        let status = if post.get::<String, _>("mode") == "auto" {
            "approved"
        } else {
            "pending"
        };
        let item=sqlx::query_scalar("INSERT INTO comments(id,school_id,post_id,author_id,parent_id,content,audit_status) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING to_jsonb(comments)").bind(cid).bind(s).bind(id).bind(a.id).bind(v.parent_id).bind(v.content.trim()).bind(status).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO notifications(id,school_id,account_id,kind,object_id) VALUES($1,$2,$3,'comment',$4)").bind(Uuid::now_v7()).bind(s).bind(post.get::<Uuid,_>("author_id")).bind(cid).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn delete_comment(&self, a: &Actor, s: Uuid, id: Uuid, revision: i64) -> Result<()> {
        let n=sqlx::query("UPDATE comments SET deleted=true,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$3 AND (author_id=$4 OR $5)").bind(s).bind(id).bind(revision).bind(a.id).bind(a.can(s,"operator")).execute(&self.0).await?.rows_affected();
        if n == 0 { Err(Error::Conflict) } else { Ok(()) }
    }
    pub async fn follow(&self, a: &Actor, s: Uuid, id: Uuid, add: bool) -> Result<()> {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM school_memberships m JOIN accounts a ON a.id=m.account_id WHERE m.school_id=$1 AND a.id=$2 AND a.active AND m.role<>'revoked')").bind(s).bind(id).fetch_one(&self.0).await?;
        if !exists {
            return Err(Error::NotFound);
        }
        if add {
            sqlx::query("WITH changed AS (INSERT INTO follows VALUES($1,$2,$3) ON CONFLICT DO NOTHING RETURNING target_id) INSERT INTO notifications(id,school_id,account_id,kind,object_id) SELECT $4,$1,target_id,'follow',$2 FROM changed")
                .bind(s)
                .bind(a.id)
                .bind(id)
                .bind(Uuid::now_v7())
                .execute(&self.0)
                .await?;
        } else {
            sqlx::query(
                "DELETE FROM follows WHERE school_id=$1 AND follower_id=$2 AND target_id=$3",
            )
            .bind(s)
            .bind(a.id)
            .bind(id)
            .execute(&self.0)
            .await?;
        }
        Ok(())
    }
    pub async fn follows(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'display_name',a.display_name) FROM follows f JOIN accounts a ON a.id=f.target_id WHERE f.school_id=$1 AND f.follower_id=$2").bind(s).bind(a.id).fetch_all(&self.0).await?)
    }
    pub async fn notifications(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Value>> {
        let (limit, offset) = page.fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT to_jsonb(n) FROM notifications n WHERE school_id=$1 AND account_id=$2 ORDER BY created_at DESC,id DESC LIMIT $3 OFFSET $4").bind(s).bind(a.id).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    pub async fn read(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE notifications SET read_at=coalesce(read_at,now()) WHERE school_id=$1 AND account_id=$2 AND id=$3").bind(s).bind(a.id).bind(id).execute(&self.0).await?;
        Ok(())
    }
    pub async fn cases(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Value>> {
        let (limit, offset) = page.fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT to_jsonb(c) FROM cases c WHERE school_id=$1 AND ($2 OR account_id=$3) ORDER BY created_at DESC,id DESC LIMIT $4 OFFSET $5").bind(s).bind(a.can(s,"operator")).bind(a.id).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    pub async fn case(&self, a: &Actor, s: Uuid, v: Value) -> Result<Value> {
        let post = v["post_id"].as_str().and_then(|x| x.parse::<Uuid>().ok());
        if let Some(id) = post {
            self.detail(a, s, id).await?;
        }
        Ok(sqlx::query_scalar("INSERT INTO cases(id,school_id,account_id,kind,post_id,content,contact,images) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING to_jsonb(cases)").bind(Uuid::now_v7()).bind(s).bind(a.id).bind(v["kind"].as_str()).bind(post).bind(v["content"].as_str()).bind(v["contact"].as_str().unwrap_or("")).bind(v.get("images").cloned().unwrap_or(json!([]))).fetch_one(&self.0).await?)
    }
    pub async fn resolve(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let item=sqlx::query_scalar("UPDATE cases SET state=$3,resolution=$4,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$5 RETURNING to_jsonb(cases)").bind(s).bind(id).bind(v["state"].as_str()).bind(v["resolution"].as_str()).bind(v["revision"].as_i64().unwrap_or(0)).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        crate::audit::record(
            &mut tx,
            a,
            s,
            "case.resolve",
            id,
            json!({"state":v["state"]}),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
    }
}

impl crate::community::south::port::Store for Repository {
    async fn validate_media(&self, s: Uuid, images: &[String]) -> Result<()> {
        Repository::validate_media(self, s, images).await
    }
    async fn check_words(&self, s: Uuid, content: &str) -> Result<()> {
        Repository::check_words(self, s, content).await
    }
    async fn posts(&self, a: &Actor, s: Uuid, p: Page) -> Result<Vec<Value>> {
        Repository::posts(self, a, s, p).await
    }
    async fn detail(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<Value> {
        Repository::detail(self, a, s, id).await
    }
    async fn create(&self, a: &Actor, s: Uuid, v: PostInput) -> Result<Value> {
        Repository::create(self, a, s, v).await
    }
    async fn update(&self, a: &Actor, s: Uuid, id: Uuid, v: PostInput) -> Result<Value> {
        Repository::update(self, a, s, id, v).await
    }
    async fn visibility(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        Repository::visibility(self, a, s, id, v).await
    }
    async fn moderate(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Decision,
        comment: bool,
    ) -> Result<Value> {
        Repository::moderate(self, a, s, id, v, comment).await
    }
    async fn react(&self, a: &Actor, s: Uuid, id: Uuid, kind: &str, add: bool) -> Result<()> {
        Repository::react(self, a, s, id, kind, add).await
    }
    async fn reactions(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        Repository::reactions(self, a, s).await
    }
    async fn comments(&self, a: &Actor, s: Uuid, id: Option<Uuid>, p: Page) -> Result<Vec<Value>> {
        Repository::comments(self, a, s, id, p).await
    }
    async fn comment(&self, a: &Actor, s: Uuid, id: Uuid, v: CommentInput) -> Result<Value> {
        Repository::comment(self, a, s, id, v).await
    }
    async fn delete_comment(&self, a: &Actor, s: Uuid, id: Uuid, revision: i64) -> Result<()> {
        Repository::delete_comment(self, a, s, id, revision).await
    }
    async fn follow(&self, a: &Actor, s: Uuid, id: Uuid, add: bool) -> Result<()> {
        Repository::follow(self, a, s, id, add).await
    }
    async fn follows(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        Repository::follows(self, a, s).await
    }
    async fn notifications(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Value>> {
        Repository::notifications(self, a, s, page).await
    }
    async fn read(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<()> {
        Repository::read(self, a, s, id).await
    }
    async fn cases(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Value>> {
        Repository::cases(self, a, s, page).await
    }
    async fn case(&self, a: &Actor, s: Uuid, v: Value) -> Result<Value> {
        Repository::case(self, a, s, v).await
    }
    async fn resolve(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        Repository::resolve(self, a, s, id, v).await
    }
}
