//! End-to-end tests of Kalks Circle over HTTP against a real PostgreSQL and mocked upstreams (tests/common).

mod common;

use common::{Env, Opts, env, env_with, png};
use reqwest::Method;
use serde_json::{Value, json};

use std::sync::atomic::Ordering;

const ANA: i64 = 100; // uid % 4 == 0 → "Ana Silva"
const BEN: i64 = 101;
const CHEN: i64 = 102;
const DARA: i64 = 103;

fn ids(v: &Value) -> Vec<i64> {
    v["items"].as_array().map(|a| a.iter().filter_map(|x| x["id"].as_i64()).collect()).unwrap_or_default()
}

// ---------------------------------------------------------------- profiles, follows, privacy, blocks

#[tokio::test]
async fn profiles_follows_privacy_and_blocks() {
    let Some(e) = env().await else { return };
    // first call creates the profile with a suggested handle and the first name
    let (s, me) = e.get("/v1/circle/me", ANA).await;
    assert_eq!(s, 200, "{me}");
    assert_eq!(me["handle"], "ana_silva");
    assert_eq!(me["displayName"], "Ana");
    assert!(me["badges"].as_array().unwrap().contains(&json!("verified")), "KYC badge from the session");
    assert!(me["settings"].is_object() && me["counters"].is_object() && me["limits"]["photosPerPost"] == 10);
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.member(CHEN, "chen").await;
    // handles: unique, validated, reserved words refused
    let (s, v) = e.patch("/v1/circle/me", BEN, json!({"handle": "ANA"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("handle_taken")));
    let (s, _) = e.patch("/v1/circle/me", BEN, json!({"handle": "kalks_support"})).await;
    assert_eq!(s, 422);
    let (_, v) = e.get("/v1/circle/handles/check?handle=ana", BEN).await;
    assert_eq!((v["valid"].as_bool(), v["available"].as_bool()), (Some(true), Some(false)));
    // public follow
    let (s, v) = e.post("/v1/circle/profiles/ana/follow", BEN, json!({})).await;
    assert_eq!((s, v["status"].as_str()), (200, Some("active")));
    let (_, ana) = e.get("/v1/circle/profiles/ana", BEN).await;
    assert_eq!(ana["profile"]["counts"]["followers"], 1);
    assert_eq!(ana["profile"]["relationship"]["following"], true);
    // private profile: follow request, content hidden until accepted
    e.patch("/v1/circle/me", CHEN, json!({"private": true})).await;
    let chen_post = e.post_text(CHEN, "Private thoughts on $EURUSD").await;
    let (_, v) = e.post("/v1/circle/profiles/chen/follow", ANA, json!({})).await;
    assert_eq!(v["status"], "requested");
    let (s, v) = e.get("/v1/circle/profiles/chen/posts", ANA).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("private_profile")));
    let (s, _) = e.get(&format!("/v1/circle/posts/{chen_post}"), ANA).await;
    assert_eq!(s, 404);
    let (_, reqs) = e.get("/v1/circle/me/requests", CHEN).await;
    assert_eq!(reqs["items"][0]["user"]["handle"], "ana");
    let (s, _) = e.post(&format!("/v1/circle/me/requests/{ANA}/accept"), CHEN, json!({})).await;
    assert_eq!(s, 200);
    let (s, v) = e.get("/v1/circle/profiles/chen/posts", ANA).await;
    assert_eq!((s, ids(&v)), (200, vec![chen_post]));
    // bell needs a follow; close friends are followers
    let (s, _) = e.post("/v1/circle/profiles/chen/bell", BEN, json!({"on": true})).await;
    assert_eq!(s, 409);
    let (s, _) = e.post("/v1/circle/profiles/ben/close-friend", CHEN, json!({})).await;
    assert_eq!(s, 409, "close friends are chosen among followers");
    let (s, v) = e.post("/v1/circle/profiles/ana/close-friend", CHEN, json!({})).await;
    assert_eq!((s, v["relationship"]["closeFriend"].as_bool()), (200, Some(true)));
    // block: follows removed both ways, profile gone for the blocked member
    let (s, _) = e.post("/v1/circle/profiles/ben/block", ANA, json!({})).await;
    assert_eq!(s, 200);
    let (s, _) = e.get("/v1/circle/profiles/ana", BEN).await;
    assert_eq!(s, 404);
    let (_, ana) = e.get("/v1/circle/profiles/ana", ANA).await;
    assert_eq!(ana["profile"]["counts"]["followers"], 0);
    let (s, _) = e.post("/v1/circle/profiles/ana/follow", BEN, json!({})).await;
    assert_eq!(s, 404);
    let (_, list) = e.get("/v1/circle/me/lists/blocked", ANA).await;
    assert_eq!(list["items"][0]["handle"], "ben");
    e.delete("/v1/circle/profiles/ben/block", ANA).await;
    let (s, _) = e.get("/v1/circle/profiles/ana", BEN).await;
    assert_eq!(s, 200);
    // restricted country (registration country or edge IP) and a broker's blocked countries
    let (s, v) = Env::json(e.user_with(Method::GET, "/v1/circle/me", 200, "kalks", &[("x-kalks-country", "ir")])).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("restricted_country")));
    let (s, _) = Env::json(e.user_with(Method::GET, "/v1/circle/me", 201, "kalks", &[("x-kalks-ip-country", "KP")])).await;
    assert_eq!(s, 403);
    let (s, _) = Env::json(e.user_with(Method::GET, "/v1/circle/me", 202, "blockedbroker", &[("x-kalks-country", "ng")])).await;
    assert_eq!(s, 403);
    let (s, _) = Env::json(e.user_of(Method::GET, "/v1/circle/me", 203, "blockedbroker")).await;
    assert_eq!(s, 200);
    // hidden words and mutes keep posts out of the feed
    e.post("/v1/circle/profiles/ben/follow", ANA, json!({})).await;
    let p1 = e.post_text(BEN, "Gold pump incoming #gold").await;
    let p2 = e.post_text(BEN, "Calm market today").await;
    let (_, f) = e.get("/v1/circle/feed/following", ANA).await;
    assert!(ids(&f).contains(&p1) && ids(&f).contains(&p2));
    Env::json(e.user(Method::PUT, "/v1/circle/me/hidden-words", ANA).json(&json!({"words": ["Pump"]}))).await;
    let (_, f) = e.get("/v1/circle/feed/following", ANA).await;
    assert!(!ids(&f).contains(&p1) && ids(&f).contains(&p2));
    e.post("/v1/circle/profiles/ben/mute", ANA, json!({"posts": true, "stories": true})).await;
    let (_, f) = e.get("/v1/circle/feed/following", ANA).await;
    assert!(!ids(&f).contains(&p2), "muted");
    // banned members can read /me (with the ban) and nothing else
    e.member(DARA, "dara").await;
    sqlx::query("UPDATE profiles SET status = 'banned', ban_reason = 'spam' WHERE user_id = $1").bind(DARA).execute(&e.st.pool).await.unwrap();
    let (s, v) = e.get("/v1/circle/me", DARA).await;
    assert_eq!((s, v["banned"]["reason"].as_str()), (200, Some("spam")));
    let (s, v) = e.get("/v1/circle/feed/following", DARA).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("banned")));
    e.drop().await;
}

// ---------------------------------------------------------------- posts and feeds

#[tokio::test]
async fn posts_rules_and_feed_ordering() {
    let Some(e) = env().await else { return };
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.member(CHEN, "chen").await;
    e.post("/v1/circle/profiles/ben/follow", ANA, json!({})).await;
    // a new post is pending until the safety check, then published (no AI: rules only)
    let (s, v) = e.post("/v1/circle/posts", BEN, json!({"body": "Long $XAUUSD above 2400 #Gold #breakout cc @chen"})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["post"]["status"], "pending");
    let p1 = v["post"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, v) = e.get(&format!("/v1/circle/posts/{p1}"), BEN).await;
    let post = &v["post"];
    assert_eq!(post["status"], "published");
    assert_eq!(post["cashtags"], json!(["XAUUSD"]));
    assert_eq!(post["hashtags"], json!(["breakout", "gold"]));
    assert_eq!(post["mentions"][0]["handle"], "chen");
    assert_eq!(post["riskLine"], "Not investment advice. Trading involves risk.");
    // links outside the allow-list and blocking keywords are refused right away
    let (s, v) = e.post("/v1/circle/posts", BEN, json!({"body": "Signals at t.me/bestsignals"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("link_not_allowed")));
    let (s, _) = e.post("/v1/circle/posts", BEN, json!({"body": "My chart https://www.tradingview.com/x/abc"})).await;
    assert_eq!(s, 200);
    let (s, v) = e.post("/v1/circle/posts", BEN, json!({"body": "This is a guaranteed profit setup!"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("content_blocked")));
    // a "review" keyword holds the post for a moderator
    let (_, v) = e.post("/v1/circle/posts", BEN, json!({"body": "WhatsApp me for the full plan"})).await;
    assert_eq!(v["post"]["status"], "review");
    let held = v["post"]["id"].as_i64().unwrap();
    let (s, _) = e.get(&format!("/v1/circle/posts/{held}"), ANA).await;
    assert_eq!(s, 404, "held posts are invisible to others");
    // staff can add a domain to the allow-list
    let (s, _) = Env::json(e.staff(Method::POST, "/v1/circle/admin/rules", "circle.read,circle.admin").json(&json!({"kind": "link_allow", "pattern": "https://www.investing.com/"}))).await;
    assert_eq!(s, 200);
    let (s, _) = e.post("/v1/circle/posts", BEN, json!({"body": "Calendar: investing.com/economic-calendar"})).await;
    assert_eq!(s, 200);
    // following feed: newest first, own and followed only, cursor paging
    let p2 = e.post_text(BEN, "Second").await;
    let p3 = e.post_text(ANA, "Mine").await;
    let other = e.post_text(CHEN, "Not followed").await;
    let (_, f) = e.get("/v1/circle/feed/following?limit=2", ANA).await;
    assert_eq!(ids(&f), vec![p3, p2]);
    let cursor = f["nextCursor"].as_str().unwrap().to_string();
    let (_, f2) = e.get(&format!("/v1/circle/feed/following?limit=50&cursor={cursor}"), ANA).await;
    assert!(ids(&f2).contains(&p1) && !ids(&f2).contains(&other) && !ids(&f2).contains(&p3));
    // For you: ranked, stable paging, followed authors first for equal posts
    let (_, fy) = e.get("/v1/circle/feed/for-you", ANA).await;
    let fy_ids = ids(&fy);
    assert!(fy_ids.contains(&other) && fy_ids.contains(&p1) && !fy_ids.contains(&p3), "own posts aren't recommended");
    assert_eq!(fy["items"][0]["author"]["handle"], "ben", "followed authors rank first");
    assert!(fy["announcements"].is_array());
    // hashtag / cashtag feeds with the price chip and crowd sentiment
    let (_, t) = e.get("/v1/circle/tags/GOLD", ANA).await;
    assert_eq!((ids(&t), t["tag"]["posts"].as_i64()), (vec![p1], Some(1)));
    let (_, c) = e.get("/v1/circle/cashtags/xauusd", ANA).await;
    assert_eq!(ids(&c), vec![p1]);
    assert_eq!(c["symbol"]["quote"]["bid"], 2410.0);
    assert_eq!(c["symbol"]["name"], "Gold");
    assert!(c["symbol"]["roomId"].as_i64().is_some(), "#gold room exists");
    // edit: re-checked, then published again; delete
    let (s, v) = e.patch(&format!("/v1/circle/posts/{p2}"), BEN, json!({"body": "Second, edited #fx"})).await;
    assert_eq!((s, v["post"]["status"].as_str()), (200, Some("pending")));
    e.settle().await;
    let (_, v) = e.get(&format!("/v1/circle/posts/{p2}"), ANA).await;
    assert_eq!((v["post"]["body"].as_str(), v["post"]["editedAt"].is_null()), (Some("Second, edited #fx"), false));
    let (s, _) = e.delete(&format!("/v1/circle/posts/{p2}"), ANA).await;
    assert_eq!(s, 403);
    e.delete(&format!("/v1/circle/posts/{p2}"), BEN).await;
    let (s, _) = e.get(&format!("/v1/circle/posts/{p2}"), ANA).await;
    assert_eq!(s, 404);
    // search and explore
    let (_, sr) = e.get("/v1/circle/search?q=be", ANA).await;
    assert_eq!(sr["users"][0]["handle"], "ben");
    let (_, sr) = e.get("/v1/circle/search?q=%23go&type=hashtags", ANA).await;
    assert_eq!(sr["hashtags"][0]["tag"], "gold");
    circle::feeds::compute_trending(&e.st).await.unwrap();
    let (s, ex) = e.get("/v1/circle/explore", ANA).await;
    assert_eq!(s, 200, "{ex}");
    assert_eq!(ex["trendingCashtags"][0]["tag"], "XAUUSD");
    assert!(ex["topics"].as_array().unwrap().len() >= 10);
    e.drop().await;
}

// ---------------------------------------------------------------- comments, reactions, reposts, saves, polls

#[tokio::test]
async fn comments_reactions_reposts_and_saves() {
    let Some(e) = env().await else { return };
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.member(CHEN, "chen").await;
    let p = e.post_text(ANA, "Gold to 2500? $XAUUSD").await;
    // comments: pending → published, counts, one level of replies
    let (s, v) = e.post(&format!("/v1/circle/posts/{p}/comments"), BEN, json!({"body": "Agree @ana"})).await;
    assert_eq!(s, 200, "{v}");
    let c1 = v["comment"]["id"].as_i64().unwrap();
    e.settle().await;
    let (s, v) = e.post(&format!("/v1/circle/posts/{p}/comments"), CHEN, json!({"body": "Reply", "parentId": c1})).await;
    assert_eq!(s, 200);
    let r1 = v["comment"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, v) = e.post(&format!("/v1/circle/posts/{p}/comments"), ANA, json!({"body": "Reply to reply", "parentId": r1})).await;
    assert_eq!(v["comment"]["parentId"], c1, "replies stay one level deep");
    e.settle().await;
    let (_, top) = e.get(&format!("/v1/circle/posts/{p}/comments"), ANA).await;
    assert_eq!(ids(&top), vec![c1]);
    assert_eq!(top["items"][0]["replies"], 2);
    let (_, replies) = e.get(&format!("/v1/circle/posts/{p}/comments?parent={c1}"), ANA).await;
    assert_eq!(replies["items"].as_array().unwrap().len(), 2);
    let (_, post) = e.get(&format!("/v1/circle/posts/{p}"), ANA).await;
    assert_eq!(post["post"]["counts"]["comments"], 3);
    // author controls: pin, followers-only, off
    let (s, _) = e.post(&format!("/v1/circle/posts/{p}/comments/{c1}/pin"), ANA, json!({})).await;
    assert_eq!(s, 200);
    let (_, top) = e.get(&format!("/v1/circle/posts/{p}/comments"), CHEN).await;
    assert_eq!(top["pinned"]["id"], c1);
    let (s, _) = e.post(&format!("/v1/circle/posts/{p}/comments/{c1}/pin"), BEN, json!({})).await;
    assert_eq!(s, 403);
    e.patch(&format!("/v1/circle/posts/{p}"), ANA, json!({"comments": "followers"})).await;
    let (s, v) = e.post(&format!("/v1/circle/posts/{p}/comments"), CHEN, json!({"body": "hi"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("comments_followers")));
    e.patch(&format!("/v1/circle/posts/{p}"), ANA, json!({"comments": "off"})).await;
    let (_, v) = e.post(&format!("/v1/circle/posts/{p}/comments"), BEN, json!({"body": "hi"})).await;
    assert_eq!(v["error"]["code"], "comments_off");
    e.patch(&format!("/v1/circle/posts/{p}"), ANA, json!({"comments": "everyone"})).await;
    // restrict: the restricted member's comments are visible to them and the author only
    e.post("/v1/circle/profiles/chen/restrict", ANA, json!({})).await;
    let (_, v) = e.post(&format!("/v1/circle/posts/{p}/comments"), CHEN, json!({"body": "restricted comment"})).await;
    let rc = v["comment"]["id"].as_i64().unwrap();
    e.settle().await;
    assert!(ids(&e.get(&format!("/v1/circle/posts/{p}/comments"), BEN).await.1).iter().all(|x| *x != rc));
    assert!(ids(&e.get(&format!("/v1/circle/posts/{p}/comments"), ANA).await.1).contains(&rc));
    // the post author may delete any comment on the post
    let (s, _) = e.delete(&format!("/v1/circle/comments/{rc}"), ANA).await;
    assert_eq!(s, 200);
    // reactions: like + bull / bear (exclusive)
    let (_, v) = e.post(&format!("/v1/circle/posts/{p}/react"), BEN, json!({"kind": "like"})).await;
    assert_eq!(v["counts"]["likes"], 1);
    e.post(&format!("/v1/circle/posts/{p}/react"), BEN, json!({"kind": "bull"})).await;
    let (_, v) = e.post(&format!("/v1/circle/posts/{p}/react"), BEN, json!({"kind": "bear"})).await;
    assert_eq!((v["counts"]["bulls"].as_i64(), v["counts"]["bears"].as_i64(), v["counts"]["likes"].as_i64()), (Some(0), Some(1), Some(1)));
    let (_, v) = e.post(&format!("/v1/circle/posts/{p}/react"), CHEN, json!({"kind": "bull"})).await;
    assert_eq!(v["counts"]["sentiment"]["bullPct"], 50.0);
    let (_, post) = e.get(&format!("/v1/circle/posts/{p}"), BEN).await;
    assert_eq!((post["post"]["viewer"]["liked"].as_bool(), post["post"]["viewer"]["vote"].as_str()), (Some(true), Some("bear")));
    let no_symbol = e.post_text(ANA, "Just a thought").await;
    let (s, _) = e.post(&format!("/v1/circle/posts/{no_symbol}/react"), BEN, json!({"kind": "bull"})).await;
    assert_eq!(s, 422, "bull / bear only on symbol posts");
    let (_, who) = e.get(&format!("/v1/circle/posts/{p}/reactions?kind=like"), ANA).await;
    assert_eq!(who["items"][0]["handle"], "ben");
    // reposts and quotes
    let (s, _) = e.post(&format!("/v1/circle/posts/{p}/repost"), BEN, json!({})).await;
    assert_eq!(s, 200);
    let (s, _) = e.post(&format!("/v1/circle/posts/{p}/repost"), BEN, json!({})).await;
    assert_eq!(s, 409);
    let (_, q) = e.post("/v1/circle/posts", CHEN, json!({"body": "Interesting", "quoteOf": p})).await;
    assert_eq!(q["post"]["kind"], "quote");
    assert_eq!(q["post"]["quoteOf"]["id"], p);
    e.settle().await;
    let (_, post) = e.get(&format!("/v1/circle/posts/{p}"), ANA).await;
    assert_eq!((post["post"]["counts"]["reposts"].as_i64(), post["post"]["counts"]["quotes"].as_i64()), (Some(1), Some(1)));
    e.delete(&format!("/v1/circle/posts/{p}/repost"), BEN).await;
    let (_, post) = e.get(&format!("/v1/circle/posts/{p}"), ANA).await;
    assert_eq!(post["post"]["counts"]["reposts"], 0);
    // saves into named collections
    let (_, col) = e.post("/v1/circle/me/collections", BEN, json!({"name": "Gold ideas"})).await;
    let cid = col["collection"]["id"].as_i64().unwrap();
    e.post(&format!("/v1/circle/posts/{p}/save"), BEN, json!({"collectionId": cid})).await;
    e.post(&format!("/v1/circle/posts/{no_symbol}/save"), BEN, json!({})).await;
    let (_, saved) = e.get(&format!("/v1/circle/me/saved?collection={cid}"), BEN).await;
    assert_eq!(ids(&saved), vec![p]);
    let (_, all) = e.get("/v1/circle/me/collections", BEN).await;
    assert_eq!((all["all"].as_i64(), all["items"][0]["posts"].as_i64()), (Some(2), Some(1)));
    // polls: one vote per member
    let (_, v) = e.post("/v1/circle/posts", ANA, json!({"body": "Where next?", "poll": {"options": ["Up", "Down", "Flat"], "durationHours": 24}})).await;
    let poll = v["post"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, v) = e.post(&format!("/v1/circle/posts/{poll}/vote"), BEN, json!({"option": 1})).await;
    assert_eq!((v["poll"]["counts"].clone(), v["poll"]["myVote"].as_i64()), (json!([0, 1, 0]), Some(1)));
    let (s, _) = e.post(&format!("/v1/circle/posts/{poll}/vote"), BEN, json!({"option": 0})).await;
    assert_eq!(s, 409);
    // activity list got the comment, the reactions and the repost
    let (_, act) = e.get("/v1/circle/me/activity", ANA).await;
    let kinds: Vec<&str> = act["items"].as_array().unwrap().iter().filter_map(|x| x["kind"].as_str()).collect();
    for k in ["comment", "like", "vote", "repost", "quote"] {
        assert!(kinds.contains(&k), "{k} in {kinds:?}");
    }
    e.drop().await;
}

// ---------------------------------------------------------------- stories

#[tokio::test]
async fn stories_expire_and_live_on_in_highlights() {
    let Some(e) = env().await else { return };
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.member(CHEN, "chen").await;
    e.post("/v1/circle/profiles/ana/follow", BEN, json!({})).await;
    e.post("/v1/circle/profiles/ana/follow", CHEN, json!({})).await;
    let (s, v) = e.post("/v1/circle/stories", ANA, json!({"kind": "text", "body": "Bull or bear on gold?", "stickers": [{"type": "sentiment", "symbol": "XAUUSD"}, {"type": "question", "prompt": "Your target?"}]})).await;
    assert_eq!(s, 200, "{v}");
    let s1 = v["story"]["id"].as_i64().unwrap();
    e.settle().await;
    // close friends story: only for the close friend
    e.post("/v1/circle/profiles/ben/close-friend", ANA, json!({})).await;
    let (_, v) = e.post("/v1/circle/stories", ANA, json!({"kind": "text", "body": "Just for friends", "audience": "close_friends"})).await;
    let s2 = v["story"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, tray) = e.get("/v1/circle/stories/tray", BEN).await;
    assert_eq!(tray["items"][0]["author"]["handle"], "ana");
    assert_eq!(tray["items"][0]["count"], 2);
    let (_, ben_view) = e.get("/v1/circle/stories/users/ana", BEN).await;
    let (_, chen_view) = e.get("/v1/circle/stories/users/ana", CHEN).await;
    assert_eq!((ids(&ben_view), ids(&chen_view)), (vec![s1, s2], vec![s1]));
    // views, sentiment votes, question answers
    e.post(&format!("/v1/circle/stories/{s1}/view"), BEN, json!({})).await;
    let (_, v) = e.post(&format!("/v1/circle/stories/{s1}/vote"), BEN, json!({"choice": "bull"})).await;
    assert_eq!(v["sentiment"]["bulls"], 1);
    let (s, _) = e.post(&format!("/v1/circle/stories/{s1}/vote"), BEN, json!({"choice": "bear"})).await;
    assert_eq!(s, 409);
    e.post(&format!("/v1/circle/stories/{s1}/answer"), CHEN, json!({"body": "2500"})).await;
    let (_, viewers) = e.get(&format!("/v1/circle/stories/{s1}/viewers"), ANA).await;
    assert_eq!((viewers["count"].as_i64(), viewers["items"][0]["vote"].as_str()), (Some(1), Some("bull")));
    let (_, answers) = e.get(&format!("/v1/circle/stories/{s1}/answers"), ANA).await;
    assert_eq!(answers["items"][0]["body"], "2500");
    let (s, _) = e.get(&format!("/v1/circle/stories/{s1}/viewers"), BEN).await;
    assert_eq!(s, 404, "only the author sees viewers");
    // reply to a story lands in the author's DMs
    let (s, v) = e.post(&format!("/v1/circle/stories/{s1}/reply"), CHEN, json!({"body": "Nice call"})).await;
    assert_eq!((s, v["message"]["kind"].as_str()), (200, Some("story_reply")));
    // highlights keep a story after the 24 hours
    let (s, h) = e.post("/v1/circle/highlights", ANA, json!({"title": "Gold calls", "storyIds": [s1]})).await;
    assert_eq!(s, 200, "{h}");
    let hid = h["highlight"]["id"].as_i64().unwrap();
    sqlx::query("UPDATE stories SET expires_at = now() - interval '1 minute'").execute(&e.st.pool).await.unwrap();
    let (_, tray) = e.get("/v1/circle/stories/tray", BEN).await;
    assert!(tray["items"].as_array().unwrap().is_empty(), "expired stories leave the tray");
    let (s, _) = e.get(&format!("/v1/circle/stories/{s2}"), BEN).await;
    assert_eq!(s, 404);
    let (_, hl) = e.get(&format!("/v1/circle/highlights/{hid}"), CHEN).await;
    assert_eq!(ids(&json!({"items": hl["highlight"]["items"]})), vec![s1]);
    let (_, list) = e.get("/v1/circle/profiles/ana/highlights", CHEN).await;
    assert_eq!((list["items"][0]["title"].as_str(), list["items"][0]["count"].as_i64()), (Some("Gold calls"), Some(1)));
    let (s, _) = e.get(&format!("/v1/circle/stories/{s1}"), CHEN).await;
    assert_eq!(s, 200, "visible through the highlight");
    let (_, arch) = e.get("/v1/circle/me/stories/archive", ANA).await;
    assert_eq!(ids(&arch), vec![s2, s1]);
    e.drop().await;
}

// ---------------------------------------------------------------- chat

#[tokio::test]
async fn chat_requests_groups_rooms_and_audited_staff_access() {
    let Some(e) = env().await else { return };
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.member(CHEN, "chen").await;
    // a stranger's DM lands in Requests
    let (s, v) = e.post("/v1/circle/chat/dm", BEN, json!({"handle": "ana"})).await;
    assert_eq!(s, 200, "{v}");
    let dm = v["conversation"]["id"].as_i64().unwrap();
    let (s, m) = e.post(&format!("/v1/circle/chat/conversations/{dm}/messages"), BEN, json!({"body": "Hi Ana, love your gold calls", "clientId": "c-1"})).await;
    assert_eq!(s, 200, "{m}");
    let m1 = m["message"]["id"].as_i64().unwrap();
    // the same clientId is idempotent
    let (_, again) = e.post(&format!("/v1/circle/chat/conversations/{dm}/messages"), BEN, json!({"body": "Hi Ana, love your gold calls", "clientId": "c-1"})).await;
    assert_eq!(again["message"]["id"], m1);
    let (_, inbox) = e.get("/v1/circle/chat/conversations", ANA).await;
    assert!(ids(&inbox).is_empty());
    let (_, req) = e.get("/v1/circle/chat/conversations?box=requests", ANA).await;
    assert_eq!((ids(&req), req["requests"].as_i64()), (vec![dm], Some(1)));
    assert_eq!(req["items"][0]["other"]["handle"], "ben");
    let (_, msgs) = e.get(&format!("/v1/circle/chat/conversations/{dm}/messages"), ANA).await;
    assert_eq!(msgs["items"][0]["body"], "Hi Ana, love your gold calls");
    // the request notification went out once (bell)
    e.deliver().await;
    assert_eq!(e.mock.notifies.lock().unwrap().iter().filter(|(_, b)| b["type"] == "circle.dm_request").count(), 1);
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{dm}/accept"), ANA, json!({})).await;
    assert_eq!(s, 200);
    let (_, inbox) = e.get("/v1/circle/chat/conversations", ANA).await;
    assert_eq!((ids(&inbox), inbox["unread"].as_i64()), (vec![dm], Some(1)));
    // read receipts
    e.post(&format!("/v1/circle/chat/conversations/{dm}/read"), ANA, json!({"messageId": m1})).await;
    let (_, c) = e.get(&format!("/v1/circle/chat/conversations/{dm}"), BEN).await;
    let ana_read = c["conversation"]["members"].as_array().unwrap().iter().find(|x| x["user"]["handle"] == "ana").unwrap()["lastReadId"].as_i64();
    assert_eq!(ana_read, Some(m1));
    // edit within 15 minutes, delete for everyone (kept for compliance)
    let (s, v) = e.patch(&format!("/v1/circle/chat/messages/{m1}"), BEN, json!({"body": "Hi Ana!"})).await;
    assert_eq!((s, v["message"]["body"].as_str()), (200, Some("Hi Ana!")));
    let (s, _) = e.delete(&format!("/v1/circle/chat/messages/{m1}"), ANA).await;
    assert_eq!(s, 403);
    e.delete(&format!("/v1/circle/chat/messages/{m1}"), BEN).await;
    let (_, msgs) = e.get(&format!("/v1/circle/chat/conversations/{dm}/messages"), ANA).await;
    assert_eq!((msgs["items"][0]["deleted"].as_bool(), msgs["items"][0]["body"].as_str()), (Some(true), Some("")));
    // links are checked in chat too
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{dm}/messages"), ANA, json!({"body": "join t.me/x"})).await;
    assert_eq!(s, 422);
    // blocking stops DMs
    e.post("/v1/circle/profiles/ben/block", ANA, json!({})).await;
    let (s, v) = e.post(&format!("/v1/circle/chat/conversations/{dm}/messages"), BEN, json!({"body": "hello?"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("blocked")));
    e.delete("/v1/circle/profiles/ben/block", ANA).await;
    // dm policy "none"
    e.patch("/v1/circle/me", CHEN, json!({"dmPolicy": "none"})).await;
    let (s, v) = e.post("/v1/circle/chat/dm", ANA, json!({"handle": "chen"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("dms_off")));
    e.patch("/v1/circle/me", CHEN, json!({"dmPolicy": "everyone"})).await;
    // groups: followers join directly, others get a request
    e.post("/v1/circle/profiles/ana/follow", BEN, json!({})).await;
    let (s, g) = e.post("/v1/circle/chat/groups", ANA, json!({"title": "Gold desk", "members": ["ben", "chen"]})).await;
    assert_eq!(s, 200, "{g}");
    let gid = g["conversation"]["id"].as_i64().unwrap();
    let states: Vec<(String, String)> = g["conversation"]["members"].as_array().unwrap().iter().map(|m| (m["user"]["handle"].as_str().unwrap().to_string(), m["state"].as_str().unwrap().to_string())).collect();
    assert!(states.contains(&("ben".into(), "active".into())) && states.contains(&("chen".into(), "request".into())));
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{gid}/messages"), BEN, json!({"body": "hello group"})).await;
    assert_eq!(s, 200);
    sqlx::query("INSERT INTO settings (key, data) VALUES ('general', '{\"maxGroupMembers\": 3}') ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data").execute(&e.st.pool).await.unwrap();
    e.st.cache.put("settings", Value::Null);
    e.member(DARA, "dara").await;
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{gid}/members"), ANA, json!({"members": ["dara"]})).await;
    assert_eq!(s, 422, "group limit");
    // public symbol rooms: preview, join, post; no paid rooms
    let (_, rooms) = e.get("/v1/circle/chat/rooms", CHEN).await;
    assert_eq!(rooms["paidRooms"], false);
    let gold = rooms["symbolRooms"].as_array().unwrap().iter().find(|r| r["slug"] == "gold").unwrap()["id"].as_i64().unwrap();
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{gold}/messages"), CHEN, json!({"body": "hi"})).await;
    assert_eq!(s, 404, "join first");
    e.post(&format!("/v1/circle/chat/conversations/{gold}/join"), CHEN, json!({})).await;
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{gold}/messages"), CHEN, json!({"body": "Gold looks strong"})).await;
    assert_eq!(s, 200);
    let (_, preview) = e.get(&format!("/v1/circle/chat/conversations/{gold}/messages"), DARA).await;
    assert_eq!((preview["preview"].as_bool(), preview["items"][0]["body"].as_str()), (Some(true), Some("Gold looks strong")));
    // masters' follower rooms: only masters create them, followers join
    let (s, v) = e.post("/v1/circle/chat/master-room", ANA, json!({})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("not_master")));
    sqlx::query("UPDATE profiles SET master_id = 7, master_program = 'copy' WHERE user_id = $1").bind(ANA).execute(&e.st.pool).await.unwrap();
    let (s, v) = e.post("/v1/circle/chat/master-room", ANA, json!({})).await;
    assert_eq!(s, 200, "{v}");
    let mroom = v["conversation"]["id"].as_i64().unwrap();
    let (s, v) = e.post(&format!("/v1/circle/chat/conversations/{mroom}/join"), DARA, json!({})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("follow_required")));
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{mroom}/join"), BEN, json!({})).await;
    assert_eq!(s, 200);
    // staff read a chat only through a report or a legal request, and every read is audited
    let (s, _) = Env::json(e.staff(Method::GET, &format!("/v1/circle/admin/conversations/{dm}/messages"), "circle.read,circle.chat_access")).await;
    assert_eq!(s, 403);
    let (s, _) = Env::json(e.staff(Method::GET, "/v1/circle/admin/content/message/1", "circle.read,circle.chat_access")).await;
    assert_eq!(s, 403);
    let (_, m2) = e.post(&format!("/v1/circle/chat/conversations/{dm}/messages"), BEN, json!({"body": "Send me 500 USDT"})).await;
    let (s, rep) = e.post("/v1/circle/reports", ANA, json!({"targetKind": "message", "targetId": m2["message"]["id"], "reason": "scam"})).await;
    assert_eq!(s, 200, "{rep}");
    let rid = rep["reportId"].as_i64().unwrap();
    let (s, _) = Env::json(e.staff(Method::POST, "/v1/circle/admin/chat-access", "circle.read,circle.chat_access").json(&json!({"conversationId": gid, "basis": "report", "reportId": rid, "reason": "Scam report"}))).await;
    assert_eq!(s, 422, "the report must be about this conversation");
    let (s, _) = Env::json(e.staff(Method::POST, "/v1/circle/admin/chat-access", "circle.read,circle.chat_access").json(&json!({"conversationId": dm, "basis": "legal", "legalReference": "Court 1/2026", "reason": "Court order"}))).await;
    assert_eq!(s, 403, "legal requests need circle.admin");
    let (s, g) = Env::json(e.staff(Method::POST, "/v1/circle/admin/chat-access", "circle.read,circle.chat_access").json(&json!({"conversationId": dm, "basis": "report", "reportId": rid, "reason": "Scam report on a message"}))).await;
    assert_eq!(s, 200, "{g}");
    let (s, msgs) = Env::json(e.staff(Method::GET, &format!("/v1/circle/admin/conversations/{dm}/messages"), "circle.read,circle.chat_access")).await;
    assert_eq!(s, 200, "{msgs}");
    let bodies: Vec<&str> = msgs["items"].as_array().unwrap().iter().filter_map(|m| m["body"].as_str()).collect();
    assert!(bodies.contains(&"Send me 500 USDT") && bodies.contains(&"Hi Ana!"), "compliance sees deleted messages too: {bodies:?}");
    let audited: Vec<String> = sqlx::query_scalar("SELECT action FROM audit_log ORDER BY id").fetch_all(&e.st.pool).await.unwrap();
    assert!(audited.contains(&"chat.access_granted".to_string()) && audited.contains(&"chat.read".to_string()));
    // the audit log is append-only
    assert!(sqlx::query("DELETE FROM audit_log").execute(&e.st.pool).await.is_err());
    // another broker's staff can't open it
    let (s, _) = Env::json(e.staff_of(Method::POST, "/v1/circle/admin/chat-access", "circle.read,circle.chat_access", "otherbroker").json(&json!({"conversationId": dm, "basis": "report", "reportId": rid, "reason": "Scam report on a message"}))).await;
    assert_eq!(s, 404);
    e.drop().await;
}

// ---------------------------------------------------------------- moderation (Claude mocked)

#[tokio::test]
async fn moderation_pipeline_with_ai() {
    let Some(e) = env_with(Opts { ai: true }).await else { return };
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    // allowed text is published after the AI check
    let ok = e.post_text(ANA, "Gold is consolidating").await;
    let (_, v) = e.get(&format!("/v1/circle/posts/{ok}"), BEN).await;
    assert_eq!(v["post"]["counts"]["likes"], 0);
    assert!(e.mock.claude.lock().unwrap().iter().any(|c| c["output_config"]["format"]["type"] == "json_schema" && c["fallbacks"] == "default"));
    // blocked by the AI: rejected with a reason, the author is told
    let (_, v) = e.post("/v1/circle/posts", ANA, json!({"body": "DM me, scamword, send USDT"})).await;
    let bad = v["post"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, mine) = e.get(&format!("/v1/circle/profiles/ana/posts"), ANA).await;
    let p = mine["items"].as_array().unwrap().iter().find(|x| x["id"] == bad).unwrap().clone();
    assert_eq!(p["status"], "rejected");
    assert!(p["reason"].as_str().unwrap().contains("scam"));
    let (s, _) = e.get(&format!("/v1/circle/posts/{bad}"), BEN).await;
    assert_eq!(s, 404);
    let (_, act) = e.get("/v1/circle/me/activity", ANA).await;
    assert!(act["items"].as_array().unwrap().iter().any(|x| x["kind"] == "moderation"));
    // review: queue → staff approves → published
    let (_, v) = e.post("/v1/circle/posts", ANA, json!({"body": "reviewword join my club"})).await;
    let held = v["post"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, q) = Env::json(e.staff(Method::GET, "/v1/circle/admin/queue", "circle.read,circle.moderate")).await;
    let item = q["items"].as_array().unwrap().iter().find(|x| x["targetId"] == held).unwrap().clone();
    assert_eq!((item["targetKind"].as_str(), item["source"].as_str()), (Some("post"), Some("ai")));
    let (s, _) = Env::json(e.staff(Method::POST, &format!("/v1/circle/admin/queue/{}/approve", item["id"]), "circle.read").json(&json!({}))).await;
    assert_eq!(s, 403, "circle.moderate needed");
    let (s, _) = Env::json(e.staff(Method::POST, &format!("/v1/circle/admin/queue/{}/approve", item["id"]), "circle.read,circle.moderate").json(&json!({"note": "fine"}))).await;
    assert_eq!(s, 200);
    let (s, _) = e.get(&format!("/v1/circle/posts/{held}"), BEN).await;
    assert_eq!(s, 200);
    // a P&L screenshot from another platform is detected and blocked; the post can't use it
    *e.mock.image_verdict.lock().unwrap() = Some(json!({"decision": "block", "categories": ["pnl_screenshot"], "reason": "MT5 balance screenshot"}));
    let bytes = png(400, 300);
    let (_, up) = e.post("/v1/circle/uploads", ANA, json!({"kind": "photo", "purpose": "post", "mime": "image/png", "size": bytes.len(), "name": "pnl.png"})).await;
    let mid = up["upload"]["id"].as_i64().unwrap();
    let (s, _) = Env::json(e.user(Method::PUT, &format!("/v1/circle/uploads/{mid}"), ANA).header("upload-offset", "0").body(bytes.clone())).await;
    assert_eq!(s, 200);
    e.settle().await;
    let (_, m) = e.get(&format!("/v1/circle/uploads/{mid}"), ANA).await;
    assert_eq!(m["status"], "rejected");
    assert!(m["media"]["reason"].as_str().unwrap().contains("verified Kalks trade cards"));
    let (s, v) = e.post("/v1/circle/posts", ANA, json!({"body": "my week", "mediaIds": [mid]})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("media_rejected")));
    // a clean photo passes
    *e.mock.image_verdict.lock().unwrap() = None;
    let (_, up) = e.post("/v1/circle/uploads", ANA, json!({"kind": "photo", "purpose": "post", "mime": "image/png", "size": bytes.len(), "name": "chart.png"})).await;
    let ok_mid = up["upload"]["id"].as_i64().unwrap();
    Env::json(e.user(Method::PUT, &format!("/v1/circle/uploads/{ok_mid}"), ANA).header("upload-offset", "0").body(bytes)).await;
    let (_, v) = e.post("/v1/circle/posts", ANA, json!({"body": "my chart", "mediaIds": [ok_mid]})).await;
    let with_photo = v["post"]["id"].as_i64().unwrap();
    assert_eq!(v["post"]["status"], "pending", "waits for its photo");
    e.settle().await;
    let (s, v) = e.get(&format!("/v1/circle/posts/{with_photo}"), BEN).await;
    assert_eq!((s, v["post"]["media"][0]["status"].as_str()), (200, Some("ready")));
    // chat text is checked after delivery and hidden when it breaks the rules
    let (_, dm) = e.post("/v1/circle/chat/dm", BEN, json!({"handle": "ana"})).await;
    let dm = dm["conversation"]["id"].as_i64().unwrap();
    let (_, m) = e.post(&format!("/v1/circle/chat/conversations/{dm}/messages"), BEN, json!({"body": "scamword pay me first"})).await;
    let mid = m["message"]["id"].as_i64().unwrap();
    e.settle().await;
    let status: String = sqlx::query_scalar("SELECT status FROM messages WHERE id = $1").bind(mid).fetch_one(&e.st.pool).await.unwrap();
    assert_eq!(status, "hidden");
    let (_, msgs) = e.get(&format!("/v1/circle/chat/conversations/{dm}/messages"), ANA).await;
    assert!(ids(&msgs).iter().all(|x| *x != mid), "hidden for the recipient");
    // translation and the AI helpers (cached, rate-limited)
    let (s, v) = e.post(&format!("/v1/circle/posts/{ok}/translate"), BEN, json!({"lang": "es"})).await;
    assert_eq!((s, v["translation"]["lang"].as_str()), (200, Some("es")));
    assert!(v["translation"]["text"].as_str().unwrap().starts_with("[translated]"));
    let calls = e.mock.claude.lock().unwrap().len();
    e.post(&format!("/v1/circle/posts/{ok}/translate"), BEN, json!({"lang": "es"})).await;
    assert_eq!(e.mock.claude.lock().unwrap().len(), calls, "cached");
    for b in ["$XAUUSD up", "$XAUUSD strong", "$XAUUSD bid"] {
        e.post_text(BEN, b).await;
    }
    let (s, v) = e.get("/v1/circle/ai/sentiment/XAUUSD", ANA).await;
    assert_eq!((s, v["summary"]["mood"].as_str()), (200, Some("bullish")));
    let (s, v) = e.post("/v1/circle/ai/caption", ANA, json!({"notes": "closed my gold long", "lang": "en"})).await;
    assert_eq!((s, v["suggestion"]["caption"].as_str()), (200, Some("Closed my $XAUUSD long at TP.")));
    e.drop().await;
}

// ---------------------------------------------------------------- trade cards (engine mocked)

fn seed_engine(e: &Env) {
    e.mock.accounts.lock().unwrap().insert(ANA, vec![json!({"login": 10000001, "userId": ANA, "type": "live", "cent": false, "currency": "USD", "balance": 1100.0, "status": "active"})]);
    e.mock.positions.lock().unwrap().insert(10000001, vec![json!({"ticket": 555, "login": 10000001, "symbol": "XAUUSD", "side": "buy", "volume": 0.5, "openPrice": 2400.0, "currentPrice": 2405.0, "sl": 2390.0, "tp": 2450.0, "openTime": "2026-10-09T08:00:00Z", "profit": 250.0, "swap": 0.0, "commission": -3.0})]);
    e.mock.deals.lock().unwrap().insert(
        10000001,
        vec![
            json!({"id": 900, "login": 10000001, "positionTicket": 444, "symbol": "EURUSD", "side": "sell", "positionSide": "buy", "entry": "out", "volume": 1.0, "price": 1.1050, "openPrice": 1.1000, "profit": 500.0, "swap": -2.0, "commission": -7.0, "reason": "tp", "time": "2026-10-08T12:00:00Z", "openTime": "2026-10-07T09:00:00Z"}),
            json!({"id": 899, "login": 10000001, "positionTicket": 444, "symbol": "EURUSD", "side": "buy", "entry": "in", "volume": 1.0, "price": 1.1000, "profit": 0.0, "time": "2026-10-07T09:00:00Z"}),
        ],
    );
}

#[tokio::test]
async fn verified_trade_cards_and_copy_this_trade() {
    let Some(e) = env().await else { return };
    seed_engine(&e);
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    Env::json(e.user_of(Method::GET, "/v1/circle/me", CHEN, "otherbroker")).await;
    let (_, src) = e.get("/v1/circle/trade-cards/sources", ANA).await;
    assert_eq!(src["accounts"][0]["positions"][0]["ticket"], 555);
    assert_eq!(src["accounts"][0]["deals"].as_array().unwrap().len(), 1, "closing deals only");
    // only the member's own engine data: another member can't share Ana's account
    let (s, _) = e.post("/v1/circle/trade-cards", BEN, json!({"login": 10000001, "ticket": 555})).await;
    assert_eq!(s, 422);
    let (s, v) = e.post("/v1/circle/trade-cards", ANA, json!({"login": 10000001, "ticket": 555, "detail": "full"})).await;
    assert_eq!(s, 200, "{v}");
    let open_card = v["tradeCard"]["id"].as_i64().unwrap();
    assert_eq!((v["tradeCard"]["verified"].as_bool(), v["tradeCard"]["state"].as_str(), v["tradeCard"]["sl"].as_f64()), (Some(true), Some("open"), Some(2390.0)));
    let (_, v) = e.post("/v1/circle/trade-cards", ANA, json!({"login": 10000001, "dealId": 900, "detail": "percent"})).await;
    let closed_card = v["tradeCard"]["id"].as_i64().unwrap();
    assert_eq!((v["tradeCard"]["pnlPct"].as_f64(), v["tradeCard"]["pips"].as_f64(), v["tradeCard"]["profit"].as_f64()), (Some(0.45), Some(50.0), Some(491.0)));
    let (s, _) = e.post("/v1/circle/trade-cards", ANA, json!({"login": 10000001, "dealId": 899})).await;
    assert_eq!(s, 422, "an opening deal isn't a result");
    // shared in posts: risk line, live P&L, the detail the owner chose
    let (_, v) = e.post("/v1/circle/posts", ANA, json!({"body": "Gold long", "tradeCardId": open_card})).await;
    let p_open = v["post"]["id"].as_i64().unwrap();
    let (_, v) = e.post("/v1/circle/posts", ANA, json!({"body": "Closed EURUSD", "tradeCardId": closed_card})).await;
    let p_closed = v["post"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, v) = e.get(&format!("/v1/circle/posts/{p_open}"), BEN).await;
    let card = &v["post"]["tradeCard"];
    assert_eq!((card["verified"].as_bool(), card["live"]["price"].as_f64(), card["live"]["pnlPct"].as_f64()), (Some(true), Some(2410.0), Some(0.42)));
    assert!(v["post"]["riskLine"].is_string());
    let (_, v) = e.get(&format!("/v1/circle/posts/{p_closed}"), BEN).await;
    assert!(v["post"]["tradeCard"]["profit"].is_null() && v["post"]["tradeCard"]["openPrice"].is_null(), "percent-only card hides money and prices");
    assert_eq!(v["post"]["tradeCard"]["pnlPct"], 0.45);
    // Copy this trade: same broker only, SL / TP on full cards
    let (s, v) = e.get(&format!("/v1/circle/posts/{p_open}/copy"), BEN).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!((v["copy"]["symbol"].as_str(), v["copy"]["side"].as_str(), v["copy"]["sl"].as_f64(), v["copy"]["tp"].as_f64()), (Some("XAUUSD"), Some("buy"), Some(2390.0), Some(2450.0)));
    assert_eq!(v["copy"]["terminalPath"], "/?symbol=XAUUSD&side=buy&sl=2390&tp=2450");
    let (_, v) = e.get(&format!("/v1/circle/posts/{p_closed}/copy"), BEN).await;
    assert!(v["copy"]["sl"].is_null());
    let (s, v) = Env::json(e.user_of(Method::GET, &format!("/v1/circle/posts/{p_open}/copy"), CHEN, "otherbroker")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("other_broker")));
    let (_, v) = Env::json(e.user_of(Method::GET, &format!("/v1/circle/posts/{p_open}"), CHEN, "otherbroker")).await;
    assert_eq!(v["post"]["copyable"], false);
    let (_, v) = e.get(&format!("/v1/circle/posts/{p_open}"), BEN).await;
    assert_eq!(v["post"]["copyable"], true);
    // a tampered snapshot no longer verifies
    sqlx::query("UPDATE trade_cards SET snapshot = jsonb_set(snapshot, '{profit}', '99999') WHERE id = $1").bind(closed_card).execute(&e.st.pool).await.unwrap();
    let (_, v) = e.get(&format!("/v1/circle/trade-cards/{closed_card}"), BEN).await;
    assert_eq!(v["tradeCard"]["verified"], false);
    // live refresh: the position closes → the card shows the final result
    e.mock.positions.lock().unwrap().insert(10000001, vec![]);
    e.mock.deals.lock().unwrap().get_mut(&10000001).unwrap().insert(0, json!({"id": 901, "login": 10000001, "positionTicket": 555, "symbol": "XAUUSD", "side": "sell", "positionSide": "buy", "entry": "out", "volume": 0.5, "price": 2450.0, "openPrice": 2400.0, "profit": 2500.0, "swap": 0.0, "commission": -3.0, "reason": "tp", "time": "2026-10-09T10:00:00Z", "openTime": "2026-10-09T08:00:00Z"}));
    let (s, v) = e.get(&format!("/v1/circle/trade-cards/{open_card}/live"), BEN).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!((v["tradeCard"]["state"].as_str(), v["tradeCard"]["closePrice"].as_f64(), v["tradeCard"]["verified"].as_bool()), (Some("closed"), Some(2450.0), Some(true)));
    e.drop().await;
}

// ---------------------------------------------------------------- media adapters

#[tokio::test]
async fn media_uploads_resumable_local_and_s3() {
    let Some(e) = env().await else { return };
    e.member(ANA, "ana").await;
    let bytes = png(1500, 900);
    let (s, up) = e.post("/v1/circle/uploads", ANA, json!({"kind": "photo", "purpose": "post", "mime": "image/png", "size": bytes.len(), "name": "chart.png"})).await;
    assert_eq!(s, 200, "{up}");
    let id = up["upload"]["id"].as_i64().unwrap();
    let token = up["upload"]["token"].as_str().unwrap().to_string();
    assert_eq!(up["upload"]["directUrl"], format!("/circle/upload/{id}"));
    let half = bytes.len() / 2;
    // first half through the BFF route
    let (s, v) = Env::json(e.user(Method::PUT, &format!("/v1/circle/uploads/{id}"), ANA).header("upload-offset", "0").body(bytes[..half].to_vec())).await;
    assert_eq!((s, v["offset"].as_u64()), (200, Some(half as u64)));
    // a wrong offset is refused with the expected one
    let (s, v) = Env::json(e.user(Method::PUT, &format!("/v1/circle/uploads/{id}"), ANA).header("upload-offset", "0").body(bytes[half..].to_vec())).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("offset_mismatch")));
    // resume directly (Caddy /circle/upload/{id}) with the upload token: HEAD gives the offset
    let head = e.http.head(format!("{}/v1/upload/{id}?token={token}", e.base)).send().await.unwrap();
    assert_eq!(head.headers()["upload-offset"].to_str().unwrap(), half.to_string());
    let bad = e.http.put(format!("{}/v1/upload/{id}?token={}", e.base, "0".repeat(48))).header("upload-offset", half.to_string()).body(bytes[half..].to_vec()).send().await.unwrap();
    assert_eq!(bad.status().as_u16(), 404);
    let r = e.http.put(format!("{}/v1/upload/{id}?token={token}", e.base)).header("upload-offset", half.to_string()).body(bytes[half..].to_vec()).send().await.unwrap();
    let v: Value = r.json().await.unwrap();
    assert_eq!((v["complete"].as_bool(), v["status"].as_str()), (Some(true), Some("processing")));
    // processing: 3 WebP sizes on local storage (no AI key, fallback "publish" in tests)
    e.settle().await;
    let (_, m) = e.get(&format!("/v1/circle/media/{id}"), ANA).await;
    assert_eq!(m["media"]["status"], "ready");
    assert_eq!((m["media"]["width"].as_i64(), m["media"]["height"].as_i64()), (Some(1500), Some(900)));
    let urls = m["media"]["urls"].as_object().unwrap();
    assert_eq!(urls.len(), 3);
    let large = urls["large"].as_str().unwrap();
    assert!(large.starts_with("/circle/media/m/") && large.ends_with("/large.webp"));
    let on_disk = std::path::Path::new(&e.st.cfg.media_dir).join(large.trim_start_matches("/circle/media/"));
    assert_eq!(&std::fs::read(on_disk).unwrap()[8..12], b"WEBP");
    // limits and types
    let (s, _) = e.post("/v1/circle/uploads", ANA, json!({"kind": "photo", "purpose": "post", "mime": "image/svg+xml", "size": 10})).await;
    assert_eq!(s, 415);
    let (s, _) = e.post("/v1/circle/uploads", ANA, json!({"kind": "file", "purpose": "chat", "mime": "application/pdf", "size": 100 * 1024 * 1024, "name": "a.pdf"})).await;
    assert_eq!(s, 413);
    let (s, _) = e.post("/v1/circle/uploads", ANA, json!({"kind": "voice", "purpose": "post", "mime": "audio/mp4", "size": 10})).await;
    assert_eq!(s, 422);
    // video without ffmpeg: a clear failure (the transcoder is unit-tested)
    let (_, up) = e.post("/v1/circle/uploads", ANA, json!({"kind": "video", "purpose": "post", "mime": "video/mp4", "size": 16})).await;
    let vid = up["upload"]["id"].as_i64().unwrap();
    Env::json(e.user(Method::PUT, &format!("/v1/circle/uploads/{vid}"), ANA).header("upload-offset", "0").body(vec![0u8; 16])).await;
    e.settle().await;
    let (_, v) = e.get(&format!("/v1/circle/uploads/{vid}"), ANA).await;
    assert_eq!(v["status"], "failed");
    assert!(v["media"]["reason"].as_str().unwrap().contains("Video uploads aren't available"));
    // a PDF in chat is sniffed (the declared type must match)
    let (_, up) = e.post("/v1/circle/uploads", ANA, json!({"kind": "file", "purpose": "chat", "mime": "application/pdf", "size": 9, "name": "plan.pdf"})).await;
    let fid = up["upload"]["id"].as_i64().unwrap();
    Env::json(e.user(Method::PUT, &format!("/v1/circle/uploads/{fid}"), ANA).header("upload-offset", "0").body(b"not a pdf".to_vec())).await;
    e.settle().await;
    let (_, v) = e.get(&format!("/v1/circle/uploads/{fid}"), ANA).await;
    assert_eq!(v["status"], "failed");
    // avatar: set once ready
    let small = png(300, 300);
    let (_, up) = e.post("/v1/circle/uploads", ANA, json!({"kind": "photo", "purpose": "avatar", "mime": "image/png", "size": small.len()})).await;
    let aid = up["upload"]["id"].as_i64().unwrap();
    Env::json(e.user(Method::PUT, &format!("/v1/circle/uploads/{aid}"), ANA).header("upload-offset", "0").body(small)).await;
    let (s, v) = e.patch("/v1/circle/me", ANA, json!({"avatarMediaId": aid})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("media_processing")));
    e.settle().await;
    let (_, v) = e.patch("/v1/circle/me", ANA, json!({"avatarMediaId": aid})).await;
    assert!(v["profile"]["avatar"]["thumb"].as_str().unwrap().ends_with("/thumb.webp"));
    // S3-compatible storage (R2) through the mock: signed PUT / GET / DELETE and CDN URLs
    let s3 = circle::storage::Storage::S3(circle::storage::S3 {
        endpoint: e.mock_url.clone(),
        bucket: "circle-media".into(),
        region: "auto".into(),
        access_key: "AKTEST".into(),
        secret_key: "secret".into(),
        public_url: "https://media.kalkstrade.com".into(),
        http: reqwest::Client::new(),
    });
    s3.put("m/2026/abc/large.webp", b"webp-bytes".to_vec()).await.unwrap();
    assert_eq!(s3.get("m/2026/abc/large.webp").await.unwrap(), b"webp-bytes");
    assert_eq!(s3.url("m/2026/abc/large.webp"), "https://media.kalkstrade.com/m/2026/abc/large.webp");
    s3.delete("m/2026/abc/large.webp").await.unwrap();
    assert!(s3.get("m/2026/abc/large.webp").await.is_err());
    assert_eq!(e.mock.s3_bad_auth.load(Ordering::SeqCst), 0, "every request was signed");
    e.drop().await;
}

// ---------------------------------------------------------------- notifications, push, rewards

#[tokio::test]
async fn notifications_batching_push_and_rewards() {
    let Some(e) = env().await else { return };
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.member(CHEN, "chen").await;
    e.member(DARA, "dara").await;
    // follow → bell
    e.post("/v1/circle/profiles/ana/follow", BEN, json!({})).await;
    e.deliver().await;
    {
        let n = e.mock.notifies.lock().unwrap();
        let (tenant, f) = n.iter().find(|(_, b)| b["type"] == "circle.follow").unwrap();
        assert_eq!((tenant.as_str(), f["userId"].as_i64(), f["title"].as_str(), f["email"].as_bool()), ("kalks", Some(ANA), Some("Ben started following you"), Some(false)));
    }
    // likes are batched per post
    let p = e.post_text(ANA, "Gold idea $XAUUSD").await;
    e.post(&format!("/v1/circle/posts/{p}/react"), BEN, json!({"kind": "like"})).await;
    e.post(&format!("/v1/circle/posts/{p}/react"), CHEN, json!({"kind": "like"})).await;
    e.post(&format!("/v1/circle/posts/{p}/react"), DARA, json!({"kind": "bull"})).await;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM notify_outbox WHERE kind IN ('circle.like','circle.vote') AND status = 'pending'").fetch_one(&e.st.pool).await.unwrap();
    assert_eq!(pending, 1, "one batched notification");
    e.deliver().await;
    {
        let n = e.mock.notifies.lock().unwrap();
        let likes: Vec<&Value> = n.iter().filter(|(_, b)| b["type"] == "circle.like").map(|(_, b)| b).collect();
        assert_eq!(likes.len(), 1);
        assert_eq!(likes[0]["title"], "Ben and 2 others reacted to your post");
    }
    // mentions and comments
    let (_, c) = e.post(&format!("/v1/circle/posts/{p}/comments"), CHEN, json!({"body": "@dara look"})).await;
    assert!(c["comment"]["id"].is_i64());
    e.settle().await;
    e.deliver().await;
    {
        let n = e.mock.notifies.lock().unwrap();
        assert!(n.iter().any(|(_, b)| b["type"] == "circle.comment" && b["userId"] == ANA));
        assert!(n.iter().any(|(_, b)| b["type"] == "circle.mention" && b["userId"] == DARA));
    }
    // preferences turn the bell off per group
    Env::json(e.user(Method::PUT, "/v1/circle/me/notification-prefs", ANA).json(&json!({"prefs": {"follows": {"bell": false, "push": false}}}))).await;
    e.post("/v1/circle/profiles/ana/follow", CHEN, json!({})).await;
    let before = e.mock.notifies.lock().unwrap().len();
    e.deliver().await;
    assert_eq!(e.mock.notifies.lock().unwrap().len(), before, "follow bell off");
    // push via FCM (service account from a file, JWT signed RS256)
    let sa = json!({
        "type": "service_account", "project_id": "kalks-app", "client_email": "circle@kalks-app.iam.gserviceaccount.com",
        "private_key": std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fcm-test-key.pem")).unwrap(),
        "token_uri": format!("{}/token", e.mock_url),
    });
    let sa_path = format!("{}/sa.json", e.dir);
    std::fs::create_dir_all(&e.dir).unwrap();
    std::fs::write(&sa_path, sa.to_string()).unwrap();
    let mut cfg = (*e.st.cfg).clone();
    cfg.fcm_service_account = sa_path;
    let st2 = circle::state::AppState { cfg: std::sync::Arc::new(cfg), ..e.st.clone() };
    let (s, _) = e.post("/v1/circle/me/devices", BEN, json!({"token": "fcm-token-ben-0123456789", "platform": "android"})).await;
    assert_eq!(s, 200);
    e.post("/v1/circle/me/devices", BEN, json!({"token": "fcm-token-ben-gone-0123456789"})).await;
    *e.mock.fcm_fail_token.lock().unwrap() = Some("fcm-token-ben-gone-0123456789".into());
    let sent = circle::push::send(&st2, BEN, "Ana posted", "Gold idea", &json!({"link": "/circle/post/1"})).await;
    assert_eq!(sent, 1);
    {
        let f = e.mock.fcm.lock().unwrap();
        assert_eq!((f[0]["project"].as_str(), f[0]["message"]["notification"]["title"].as_str(), f[0]["message"]["data"]["link"].as_str()), (Some("kalks-app"), Some("Ana posted"), Some("/circle/post/1")));
    }
    let disabled: bool = sqlx::query_scalar("SELECT disabled FROM devices WHERE token = 'fcm-token-ben-gone-0123456789'").fetch_one(&e.st.pool).await.unwrap();
    assert!(disabled, "unregistered tokens are disabled");
    assert_eq!(circle::push::send(&e.st, BEN, "x", "y", &json!({})).await, 0, "no-op until configured");
    // helpful post → Rewards points through the growth service (outbox)
    sqlx::query("INSERT INTO settings (key, data) VALUES ('general', '{\"helpfulThreshold\": 3, \"helpfulPoints\": 50}') ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data").execute(&e.st.pool).await.unwrap();
    e.st.cache.put("settings", Value::Null);
    let p2 = e.post_text(ANA, "How I size positions #risk").await;
    for u in [BEN, CHEN, DARA] {
        e.post(&format!("/v1/circle/posts/{p2}/react"), u, json!({"kind": "like"})).await;
    }
    let (_, v) = e.get(&format!("/v1/circle/posts/{p2}"), BEN).await;
    assert_eq!(v["post"]["helpful"], true);
    circle::rewards::deliver(&e.st).await.unwrap();
    {
        let g = e.mock.growth.lock().unwrap();
        assert_eq!((g[0]["userId"].as_i64(), g[0]["points"].as_i64(), g[0]["staffRole"].as_str()), (Some(ANA), Some(50), Some("finance")));
    }
    let status: String = sqlx::query_scalar("SELECT status FROM outbox WHERE dedupe_key = $1").bind(format!("helpful:{p2}")).fetch_one(&e.st.pool).await.unwrap();
    assert_eq!(status, "delivered");
    let (_, me) = e.get("/v1/circle/me", ANA).await;
    let keys: Vec<&str> = me["achievements"].as_array().unwrap().iter().filter_map(|a| a["key"].as_str()).collect();
    assert!(keys.contains(&"first_post") && keys.contains(&"helpful_1"), "{keys:?}");
    assert!(me["xp"].as_i64().unwrap() >= 50);
    e.drop().await;
}

// ---------------------------------------------------------------- module switch, stream, staff, public

#[tokio::test]
async fn module_off_stream_staff_and_public_pages() {
    let Some(e) = env().await else { return };
    // module `circle` off for the broker → 403 on every client route and the stream ticket
    for path in ["/v1/circle/me", "/v1/circle/feed/following", "/v1/circle/chat/conversations"] {
        let (s, v) = Env::json(e.user_of(Method::GET, path, 300, "offbroker")).await;
        assert_eq!((s, v["error"]["code"].as_str()), (403, Some("module_disabled")), "{path}");
    }
    let (s, v) = Env::json(e.user_of(Method::POST, "/v1/stream/ticket", 300, "offbroker")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("module_disabled")));
    // the internal token is required
    let r = e.http.get(format!("{}/v1/circle/me", e.base)).header("x-kalks-user-id", "1").send().await.unwrap();
    assert_eq!(r.status().as_u16(), 403);
    // realtime: a DM arrives over the WebSocket
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.post("/v1/circle/profiles/ben/follow", ANA, json!({})).await;
    let (_, t) = e.post("/v1/stream/ticket", ANA, json!({})).await;
    let ticket = t["ticket"].as_str().unwrap().to_string();
    let (_, dm) = e.post("/v1/circle/chat/dm", BEN, json!({"handle": "ana"})).await;
    let dm = dm["conversation"]["id"].as_i64().unwrap();
    let ws_url = format!("{}/v1/stream?ticket={ticket}", e.base.replace("http", "ws"));
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
    use futures_util::{SinkExt, StreamExt};
    let hello: Value = serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
    assert_eq!(hello["type"], "hello");
    e.post(&format!("/v1/circle/chat/conversations/{dm}/messages"), BEN, json!({"body": "realtime hi"})).await;
    let mut got = None;
    for _ in 0..10 {
        let f: Value = serde_json::from_str(&tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap().into_text().unwrap()).unwrap();
        if f["type"] == "message" {
            got = Some(f);
            break;
        }
    }
    let got = got.expect("message frame");
    assert_eq!((got["conversationId"].as_i64(), got["message"]["body"].as_str()), (Some(dm), Some("realtime hi")));
    // typing from the socket reaches the other member, not the sender
    ws.send(tokio_tungstenite::tungstenite::Message::Text(json!({"type": "typing", "conversationId": dm}).to_string().into())).await.unwrap();
    // a ticket is single-use
    assert!(tokio_tungstenite::connect_async(&ws_url).await.is_err());
    // staff: overview, users, ban with audit; broker staff limited to their clients
    let (s, o) = Env::json(e.staff(Method::GET, "/v1/circle/admin/overview", "circle.read")).await;
    assert_eq!((s, o["scope"].as_str()), (200, Some("community")));
    let (s, _) = Env::json(e.staff(Method::GET, "/v1/circle/admin/overview", "notifications.write")).await;
    assert_eq!(s, 403);
    let (s, _) = Env::json(e.staff(Method::POST, &format!("/v1/circle/admin/users/{BEN}/ban"), "circle.read,circle.moderate").json(&json!({"reason": "Spam", "days": 7}))).await;
    assert_eq!(s, 200);
    let (s, v) = e.get("/v1/circle/feed/following", BEN).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("banned")));
    let (s, _) = Env::json(e.staff_of(Method::GET, &format!("/v1/circle/admin/users/{ANA}"), "circle.read", "otherbroker")).await;
    assert_eq!(s, 404);
    let (_, users) = Env::json(e.staff_of(Method::GET, "/v1/circle/admin/users", "circle.read", "otherbroker")).await;
    assert!(users["items"].as_array().unwrap().is_empty());
    let (s, _) = Env::json(e.staff(Method::POST, &format!("/v1/circle/admin/users/{ANA}/badge"), "circle.read,circle.content").json(&json!({"badge": "mentor"}))).await;
    assert_eq!(s, 200);
    let (_, a) = e.get("/v1/circle/profiles/ana", ANA).await;
    assert_eq!(a["profile"]["badges"][0], "mentor");
    let (_, audit) = Env::json(e.staff(Method::GET, "/v1/circle/admin/audit", "circle.admin")).await;
    let actions: Vec<&str> = audit["items"].as_array().unwrap().iter().filter_map(|x| x["action"].as_str()).collect();
    assert!(actions.contains(&"user.ban") && actions.contains(&"user.badge"), "{actions:?}");
    // announcements show at the top of For you
    let (s, _) = Env::json(e.staff(Method::POST, "/v1/circle/admin/announcements", "circle.content").json(&json!({"title": "Welcome to Kalks Circle", "body": "Read the rules", "link": "/circle/rules"}))).await;
    assert_eq!(s, 200);
    let (_, fy) = e.get("/v1/circle/feed/for-you", ANA).await;
    assert_eq!(fy["announcements"][0]["title"], "Welcome to Kalks Circle");
    // public website pages: public content only, no token, nothing private
    let p = e.post_text(ANA, "Public idea #gold").await;
    let r: Value = e.http.get(format!("{}/v1/public/profiles/ana", e.base)).send().await.unwrap().json().await.unwrap();
    assert_eq!(r["profile"]["handle"], "ana");
    assert!(r["profile"].get("tenant").is_none() && r["profile"].get("relationship").is_none());
    let r: Value = e.http.get(format!("{}/v1/public/posts/{p}", e.base)).send().await.unwrap().json().await.unwrap();
    assert_eq!(r["post"]["body"], "Public idea #gold");
    assert!(r["post"].get("viewer").is_none());
    e.patch("/v1/circle/me", ANA, json!({"private": true})).await;
    let r = e.http.get(format!("{}/v1/public/posts/{p}", e.base)).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 404);
    // health
    let h: Value = e.http.get(format!("{}/health", e.base)).send().await.unwrap().json().await.unwrap();
    assert_eq!((h["status"].as_str(), h["storage"].as_str(), h["ffmpeg"].as_bool()), (Some("ok"), Some("local"), Some(false)));
    e.drop().await;
}

// ---------------------------------------------------------------- every other route answers (SQL is checked at runtime)

#[tokio::test]
async fn remaining_routes_and_jobs_answer() {
    let Some(e) = env().await else { return };
    seed_engine(&e);
    e.member(ANA, "ana").await;
    e.member(BEN, "ben").await;
    e.member(CHEN, "chen").await;
    e.post("/v1/circle/profiles/ana/follow", BEN, json!({})).await;
    e.post("/v1/circle/profiles/ana/follow", CHEN, json!({})).await;
    // verified stats from the engine, opt-in, leaderboards
    let (s, v) = e.post("/v1/circle/me/stats/refresh", ANA, json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["stats"]["trades"], 1);
    e.patch("/v1/circle/me", ANA, json!({"showStats": true, "showIbLink": true, "bio": "Gold trader", "displayName": "Ana S", "dmPolicy": "following", "commentsDefault": "followers", "lang": "hi"})).await;
    sqlx::query("UPDATE profiles SET risk_score = 1.5 WHERE user_id = $1").bind(ANA).execute(&e.st.pool).await.unwrap();
    let (_, me) = e.get("/v1/circle/me/stats", ANA).await;
    assert_eq!(me["showStats"], true);
    let (_, ana) = e.get("/v1/circle/profiles/ana", BEN).await;
    assert_eq!((ana["profile"]["stats"]["trades"].as_i64(), ana["profile"]["ibLink"]["referralCode"].as_str()), (Some(1), Some("REF100")));
    let (s, lb) = e.get("/v1/circle/leaderboards/traders", BEN).await;
    assert_eq!((s, lb["items"][0]["profile"]["handle"].as_str()), (200, Some("ana")));
    let p = e.post_text(ANA, "Risk first #risk $EURUSD").await;
    e.post(&format!("/v1/circle/posts/{p}/react"), BEN, json!({"kind": "like"})).await;
    let (s, lb) = e.get("/v1/circle/leaderboards/creators", BEN).await;
    assert_eq!((s, lb["items"][0]["profile"]["handle"].as_str()), (200, Some("ana")));
    let (s, _) = e.get("/v1/circle/leaderboards/nope", BEN).await;
    assert_eq!(s, 404);
    // background jobs
    sqlx::query("INSERT INTO settings (key, data) VALUES ('general', '{\"feeDiscountTop\": 1, \"creatorMinFollowers\": 2}') ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data").execute(&e.st.pool).await.unwrap();
    e.st.cache.put("settings", Value::Null);
    circle::rewards::creators(&e.st).await.unwrap();
    let (_, v) = e.get("/v1/circle/me", ANA).await;
    assert!(v["badges"].as_array().unwrap().contains(&json!("creator")));
    assert_eq!(v["settings"]["feeDiscountEligible"], true);
    let fd: Value = e.http.get(format!("{}/v1/internal/creators/fee-discounts", e.base)).header("x-kalks-internal", common::TOKEN).send().await.unwrap().json().await.unwrap();
    assert_eq!(fd["items"][0]["userId"], ANA);
    circle::stats::refresh_due(&e.st).await.unwrap();
    circle::media::cleanup(&e.st).await.unwrap();
    circle::profiles::refresh_badges(&e.st, ANA, "kalks").await.unwrap();
    let (_, v) = e.get("/v1/circle/me", ANA).await;
    assert!(v["badges"].as_array().unwrap().contains(&json!("live")) && v["badges"].as_array().unwrap().contains(&json!("academy")));
    // topics, topic videos (a video post inserted directly: no ffmpeg in tests), Academy threads, rules, prices, digest
    let (_, t) = e.get("/v1/circle/topics", BEN).await;
    assert_eq!(t["items"].as_array().unwrap().len(), 11);
    let vid: i64 = sqlx::query_scalar("INSERT INTO posts (author, kind, body, topic, status, published_at, notified) VALUES ($1, 'video', 'Gold basics', 'gold', 'published', now(), true) RETURNING id").bind(ANA).fetch_one(&e.st.pool).await.unwrap();
    for sort in ["new", "top"] {
        let (s, v) = e.get(&format!("/v1/circle/topics/gold/videos?sort={sort}"), BEN).await;
        assert_eq!((s, ids(&v)), (200, vec![vid]), "{sort}");
    }
    let (s, _) = e.get("/v1/circle/topics/nope/videos", BEN).await;
    assert_eq!(s, 404);
    let (_, v) = e.post("/v1/circle/posts", ANA, json!({"body": "What is leverage?", "academyChapter": "basics-leverage"})).await;
    let q = v["post"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, th) = e.get("/v1/circle/academy/basics-leverage/thread", BEN).await;
    assert_eq!((ids(&th), th["chapter"]["posts"].as_i64()), (vec![q], Some(1)));
    let (_, r) = e.get("/v1/circle/rules", BEN).await;
    assert!(r["rules"].as_str().unwrap().contains("No profit promises") && r["allowedLinks"].as_array().unwrap().len() >= 5);
    let (_, pr) = e.get("/v1/circle/prices?symbols=XAUUSD,$eurusd,bad/x", BEN).await;
    assert_eq!(pr["quotes"].as_object().unwrap().len(), 2);
    let (_, d) = e.get("/v1/circle/ai/digest", BEN).await;
    assert_eq!(d["reason"], "ai_unavailable");
    let (_, sent) = e.get("/v1/circle/ai/sentiment/EURUSD", BEN).await;
    assert!(sent["summary"].is_null() && sent["crowd"].is_object());
    let (s, _) = e.post(&format!("/v1/circle/posts/{p}/translate"), BEN, json!({"lang": "de"})).await;
    assert_eq!(s, 503, "no AI key");
    // trade-card sources and a card in a chat message
    let (_, src) = e.get("/v1/circle/trade-cards/sources", ANA).await;
    assert_eq!(src["accounts"].as_array().unwrap().len(), 1);
    let (_, card) = e.post("/v1/circle/trade-cards", ANA, json!({"login": 10000001, "dealId": 900})).await;
    let cid = card["tradeCard"]["id"].as_i64().unwrap();
    // pins, unpin, profile tabs, followers lists, activity read, prefs, devices, lists
    e.post(&format!("/v1/circle/posts/{p}/pin"), ANA, json!({})).await;
    let (_, tab) = e.get("/v1/circle/profiles/ana/posts", BEN).await;
    assert_eq!(tab["pinned"]["id"], p);
    e.delete(&format!("/v1/circle/posts/{p}/pin"), ANA).await;
    for t in ["media", "trades", "videos", "reposts"] {
        let (s, _) = e.get(&format!("/v1/circle/profiles/ana/posts?tab={t}"), BEN).await;
        assert_eq!(s, 200, "{t}");
    }
    let (_, f) = e.get("/v1/circle/profiles/ana/followers", ANA).await;
    assert_eq!(f["items"].as_array().unwrap().len(), 2);
    let (_, f) = e.get("/v1/circle/profiles/ben/following", ANA).await;
    assert_eq!(f["items"][0]["handle"], "ana");
    let (_, a) = e.post("/v1/circle/me/activity/read", ANA, json!({})).await;
    assert_eq!(a["unread"], 0);
    let (_, pf) = e.get("/v1/circle/me/notification-prefs", ANA).await;
    assert_eq!(pf["prefs"]["moderation"]["locked"], true);
    let (s, _) = Env::json(e.user(Method::DELETE, "/v1/circle/me/devices", ANA).json(&json!({"token": "x"}))).await;
    assert_eq!(s, 200);
    for l in ["muted", "restricted", "close-friends"] {
        let (s, _) = e.get(&format!("/v1/circle/me/lists/{l}"), ANA).await;
        assert_eq!(s, 200);
    }
    e.post(&format!("/v1/circle/me/followers/{CHEN}/remove"), ANA, json!({})).await;
    let (_, ana) = e.get("/v1/circle/profiles/ana", ANA).await;
    assert_eq!(ana["profile"]["counts"]["followers"], 1);
    // collections rename / delete, comment edit / like / translate
    let (_, c) = e.post("/v1/circle/me/collections", BEN, json!({"name": "A"})).await;
    let col = c["collection"]["id"].as_i64().unwrap();
    let (s, _) = e.patch(&format!("/v1/circle/me/collections/{col}"), BEN, json!({"name": "B"})).await;
    assert_eq!(s, 200);
    e.delete(&format!("/v1/circle/me/collections/{col}"), BEN).await;
    let (_, cm) = e.post(&format!("/v1/circle/posts/{p}/comments"), BEN, json!({"body": "nice"})).await;
    let cmid = cm["comment"]["id"].as_i64().unwrap();
    e.settle().await;
    let (s, v) = e.patch(&format!("/v1/circle/comments/{cmid}"), BEN, json!({"body": "very nice"})).await;
    assert_eq!((s, v["comment"]["status"].as_str()), (200, Some("pending")));
    e.settle().await;
    let (_, v) = e.post(&format!("/v1/circle/comments/{cmid}/like"), ANA, json!({})).await;
    assert_eq!(v["likes"], 1);
    e.delete(&format!("/v1/circle/comments/{cmid}/like"), ANA).await;
    let (_, post) = e.get(&format!("/v1/circle/posts/{p}"), ANA).await;
    assert_eq!(post["post"]["counts"]["comments"], 1, "an edited comment is counted once");
    // chat: group admin actions, members, mute, leave, edit, a trade card and a shared post in a message
    let (_, g) = e.post("/v1/circle/chat/groups", ANA, json!({"title": "Desk", "members": ["ben"]})).await;
    let gid = g["conversation"]["id"].as_i64().unwrap();
    let (s, _) = e.patch(&format!("/v1/circle/chat/conversations/{gid}"), ANA, json!({"title": "Gold desk", "about": "Daily ideas"})).await;
    assert_eq!(s, 200);
    let (s, _) = e.patch(&format!("/v1/circle/chat/conversations/{gid}"), BEN, json!({"title": "x"})).await;
    assert_eq!(s, 403);
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{gid}/admins/{BEN}"), ANA, json!({})).await;
    assert_eq!(s, 200);
    let (_, mem) = e.get(&format!("/v1/circle/chat/conversations/{gid}/members"), BEN).await;
    assert_eq!(mem["items"].as_array().unwrap().len(), 2);
    let (s, m) = e.post(&format!("/v1/circle/chat/conversations/{gid}/messages"), ANA, json!({"body": "my trade", "tradeCardId": cid})).await;
    assert_eq!((s, m["message"]["kind"].as_str()), (200, Some("trade_card")));
    let (_, m) = e.post(&format!("/v1/circle/chat/conversations/{gid}/messages"), BEN, json!({"postId": p})).await;
    assert_eq!(m["message"]["kind"], "post");
    let (s, _) = e.get(&format!("/v1/circle/trade-cards/{cid}"), BEN).await;
    assert_eq!(s, 200, "visible through the chat");
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{gid}/mute"), BEN, json!({})).await;
    assert_eq!(s, 200);
    e.post(&format!("/v1/circle/chat/conversations/{gid}/typing"), BEN, json!({})).await;
    let (_, list) = e.get("/v1/circle/chat/conversations?box=inbox", BEN).await;
    assert_eq!(list["items"][0]["me"]["muted"], true);
    let (s, _) = e.post(&format!("/v1/circle/chat/conversations/{gid}/leave"), ANA, json!({})).await;
    assert_eq!(s, 200);
    let owner: String = sqlx::query_scalar("SELECT role FROM conv_members WHERE conversation_id = $1 AND user_id = $2").bind(gid).bind(BEN).fetch_one(&e.st.pool).await.unwrap();
    assert_eq!(owner, "owner", "the group was handed over");
    let (s, _) = Env::json(e.user(Method::DELETE, &format!("/v1/circle/chat/conversations/{gid}/members/{BEN}"), BEN)).await;
    assert_eq!(s, 200);
    let (_, rooms) = e.get("/v1/circle/chat/rooms", BEN).await;
    let gold = rooms["symbolRooms"][0]["id"].as_i64().unwrap();
    let (s, _) = e.get(&format!("/v1/circle/chat/conversations/{gold}"), CHEN).await;
    assert_eq!(s, 200, "rooms can be previewed");
    // stories: delete; highlights: edit and delete
    let (_, st) = e.post("/v1/circle/stories", ANA, json!({"kind": "text", "body": "hello"})).await;
    let sid = st["story"]["id"].as_i64().unwrap();
    e.settle().await;
    let (_, h) = e.post("/v1/circle/highlights", ANA, json!({"title": "H", "storyIds": [sid]})).await;
    let hid = h["highlight"]["id"].as_i64().unwrap();
    let (s, v) = e.patch(&format!("/v1/circle/highlights/{hid}"), ANA, json!({"title": "Hi", "position": 2})).await;
    assert_eq!((s, v["highlight"]["title"].as_str()), (200, Some("Hi")));
    let (s, _) = e.delete(&format!("/v1/circle/stories/{sid}"), BEN).await;
    assert_eq!(s, 403);
    e.delete(&format!("/v1/circle/stories/{sid}"), ANA).await;
    e.delete(&format!("/v1/circle/highlights/{hid}"), ANA).await;
    let (_, hl) = e.get("/v1/circle/profiles/ana/highlights", BEN).await;
    assert!(hl["items"].as_array().unwrap().is_empty());
    // reports → Back Office: overview, reports, resolve, content, actions, users, rules, announcements, topics,
    // features, creators, settings, outbox
    let (_, rep) = e.post("/v1/circle/reports", BEN, json!({"targetKind": "post", "targetId": p, "reason": "spam", "note": "ad"})).await;
    let rid = rep["reportId"].as_i64().unwrap();
    let staff = |m: Method, path: &str| e.staff(m, path, "circle.read,circle.moderate,circle.content,circle.admin,circle.chat_access");
    let (_, o) = Env::json(staff(Method::GET, "/v1/circle/admin/overview")).await;
    assert_eq!(o["reports"], 1);
    let (_, r) = Env::json(staff(Method::GET, "/v1/circle/admin/reports")).await;
    assert_eq!(r["items"][0]["reason"], "spam");
    let (s, _) = Env::json(staff(Method::POST, &format!("/v1/circle/admin/reports/{rid}/resolve")).json(&json!({"action": "warn", "reason": "Advertising"}))).await;
    assert_eq!(s, 200);
    let w: i32 = sqlx::query_scalar("SELECT warnings FROM profiles WHERE user_id = $1").bind(ANA).fetch_one(&e.st.pool).await.unwrap();
    assert_eq!(w, 1);
    let (s, c) = Env::json(staff(Method::GET, &format!("/v1/circle/admin/content/post/{p}"))).await;
    assert_eq!((s, c["item"]["id"].as_i64()), (200, Some(p)));
    for action in ["feature", "helpful", "shadow", "unshadow", "unfeature", "remove", "restore"] {
        let (s, v) = Env::json(staff(Method::POST, &format!("/v1/circle/admin/content/post/{p}/{action}")).json(&json!({"hours": 24}))).await;
        assert_eq!(s, 200, "{action}: {v}");
    }
    let (s, _) = e.get(&format!("/v1/circle/posts/{p}"), CHEN).await;
    assert_eq!(s, 200, "restored");
    let (_, u) = Env::json(staff(Method::GET, "/v1/circle/admin/users?q=an")).await;
    assert!(u["items"].as_array().unwrap().iter().any(|x| x["handle"] == "ana"));
    let (_, u) = Env::json(staff(Method::GET, &format!("/v1/circle/admin/users/{ANA}"))).await;
    assert_eq!(u["user"]["warnings"], 1);
    for (action, body) in [("shadow", json!({})), ("unshadow", json!({})), ("creator", json!({"on": true})), ("fee-discount", json!({"eligible": false}))] {
        let (s, v) = Env::json(staff(Method::POST, &format!("/v1/circle/admin/users/{BEN}/{action}")).json(&body)).await;
        assert_eq!(s, 200, "{action}: {v}");
    }
    let (_, rules) = Env::json(staff(Method::GET, "/v1/circle/admin/rules")).await;
    let rule = rules["items"].as_array().unwrap().iter().find(|r| r["pattern"] == "whatsapp me").unwrap()["id"].as_i64().unwrap();
    let (s, v) = Env::json(staff(Method::PATCH, &format!("/v1/circle/admin/rules/{rule}")).json(&json!({"action": "block", "active": false}))).await;
    assert_eq!((s, v["rule"]["active"].as_bool()), (200, Some(false)));
    let (s, _) = Env::json(staff(Method::DELETE, &format!("/v1/circle/admin/rules/{rule}"))).await;
    assert_eq!(s, 200);
    let (s, _) = Env::json(staff(Method::POST, "/v1/circle/admin/rules").json(&json!({"kind": "keyword", "pattern": "pump and dump", "action": "review"}))).await;
    assert_eq!(s, 200);
    let (_, an) = Env::json(staff(Method::POST, "/v1/circle/admin/announcements").json(&json!({"title": "T", "link": "/circle/rules"}))).await;
    let aid = an["announcement"]["id"].as_i64().unwrap();
    let (s, _) = Env::json(staff(Method::PATCH, &format!("/v1/circle/admin/announcements/{aid}")).json(&json!({"title": "T2", "pinned": false}))).await;
    assert_eq!(s, 200);
    let (s, _) = Env::json(staff(Method::POST, "/v1/circle/admin/announcements").json(&json!({"title": "bad", "link": "javascript:alert(1)"}))).await;
    assert_eq!(s, 422);
    Env::json(staff(Method::DELETE, &format!("/v1/circle/admin/announcements/{aid}"))).await;
    let (_, list) = Env::json(staff(Method::GET, "/v1/circle/admin/announcements")).await;
    assert!(!list["items"][0]["endsAt"].is_null());
    let (s, _) = Env::json(staff(Method::POST, "/v1/circle/admin/topics").json(&json!({"key": "macro", "title": "Macro", "position": 12}))).await;
    assert_eq!(s, 200);
    let (_, tp) = Env::json(staff(Method::GET, "/v1/circle/admin/topics")).await;
    assert_eq!(tp["items"].as_array().unwrap().len(), 12);
    let (s, _) = Env::json(staff(Method::POST, "/v1/circle/admin/features").json(&json!({"userId": BEN, "days": 7}))).await;
    assert_eq!(s, 200);
    let (_, ft) = Env::json(staff(Method::GET, "/v1/circle/admin/features")).await;
    assert_eq!(ft["items"][0]["profile"]["handle"], "ben");
    let (_, ex) = e.get("/v1/circle/explore", CHEN).await;
    assert_eq!(ex["creators"][0]["profile"]["handle"], "ben", "featured creators first");
    Env::json(staff(Method::DELETE, &format!("/v1/circle/admin/features/{BEN}"))).await;
    let (s, _) = Env::json(staff(Method::GET, "/v1/circle/admin/creators")).await;
    assert_eq!(s, 200);
    let (s, v) = Env::json(staff(Method::PUT, "/v1/circle/admin/settings").json(&json!({"restrictedCountries": ["RU", "x"], "maxGroupMembers": 500, "unknown": 1}))).await;
    assert_eq!((s, v["settings"]["restrictedCountries"].clone(), v["settings"]["maxGroupMembers"].as_i64()), (200, json!(["ru"]), Some(100)));
    let (s, _) = Env::json(e.user_with(Method::GET, "/v1/circle/me", 400, "kalks", &[("x-kalks-country", "ru")])).await;
    assert_eq!(s, 403, "Back Office restricted countries apply at once");
    let (_, set) = Env::json(staff(Method::GET, "/v1/circle/admin/settings")).await;
    assert_eq!(set["env"]["aiModel"], "claude-opus-5-5");
    let (_, ob) = Env::json(staff(Method::GET, "/v1/circle/admin/outbox")).await;
    let oid = ob["items"][0]["id"].as_i64().unwrap();
    sqlx::query("UPDATE outbox SET status = 'failed'").execute(&e.st.pool).await.unwrap();
    let (s, _) = Env::json(staff(Method::POST, &format!("/v1/circle/admin/outbox/{oid}/retry"))).await;
    assert_eq!(s, 200);
    let (_, grants) = Env::json(staff(Method::GET, "/v1/circle/admin/chat-access")).await;
    assert!(grants["items"].as_array().unwrap().is_empty());
    let (_, q) = Env::json(staff(Method::GET, "/v1/circle/admin/queue?status=removed")).await;
    assert!(q["items"].is_array());
    e.drop().await;
}
