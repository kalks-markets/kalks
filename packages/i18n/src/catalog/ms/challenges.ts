import type { NsMessages } from "../../core";

// Challenges (phase 4): Rewards hub, leaderboards, nickname, Kalks Trader rank widget.
// Not translated yet: these strings are the English source (the parity check needs every key).
const challenges: NsMessages<"challenges"> = {
  // Level and XP
  "level.title": "Your level",
  "level.subtitle": "Earn XP from missions, contests and streaks",
  "level.levelN": "Level {level}",
  "level.xp": "{xp} XP",
  "level.progress": "Progress to the next level",
  "level.toNext": "{xp} XP to {name}",
  "level.top": "Top level reached",
  "level.short": "Level {level} · {name}",

  // Streaks
  "streak.days": { one: "{count} day", other: "{count} days" },
  "streak.best": "Best {count}",
  "streak.trading": "Trading streak",
  "streak.tradingHint": "+{xp} XP every 7 days",
  "streak.login": "Login streak",
  "streak.loginHint": "Open the app daily",

  // Missions
  "missions.title": "Missions",
  "missions.subtitle": "Daily, weekly and one-off goals",
  "missions.emptyTitle": "No missions right now",
  "missions.emptyText": "New missions appear here. Keep trading to keep your streak.",
  "missions.rules": "Only live trades count: at least 0.01 lots, held at least a minute. Each mission rewards once per day, week or ever.",
  "missions.open": { one: "{count} mission open", other: "{count} missions open" },
  "mission.daily": "Daily",
  "mission.weekly": "Weekly",
  "mission.once": "One-off",
  "mission.done": "Done",
  "mission.resetsIn": "Resets in {time}",
  "mission.cash": "{amount} cash",
  "mission.credit": "{amount} credit",

  // Reward states
  "reward.kyc": "Verify KYC to receive it",
  "reward.approval": "Being reviewed",
  "reward.pending": "Paying",
  "reward.paid": "Paid",
  "reward.failed": "Retrying",
  "reward.rejected": "Not paid",

  // Units for mission progress
  "unit.lots": "lots",
  "unit.days": "days",
  "unit.hours": "h",
  "unit.usd": "USD",
  "unit.friends": "friends",
  "unit.contests": "contests",

  // Recent rewards
  "recent.title": "Recent rewards",
  "recent.subtitle": "XP, prizes and mission rewards",
  "recent.emptyTitle": "Nothing yet",
  "recent.emptyText": "Complete a mission or join a contest to earn your first XP.",

  // Leaderboards
  "board.title": "Leaderboard",
  "board.subtitle": "Traders ranked by XP or by contest score. Everyone appears by nickname.",
  "board.back": "Rewards",
  "board.open": "Leaderboard",
  "board.nickname": "Your nickname",
  "board.pick": "Choose a leaderboard",
  "board.week": "This week",
  "board.month": "This month",
  "board.all": "All time",
  "board.xpWeek": "XP this week",
  "board.xpMonth": "XP this month",
  "board.xpAll": "XP all time",
  "board.xpHint": "XP from missions, contests and trading streaks",
  "board.rankedBy": "Ranked by {scoring}",
  "board.emptyTitle": "No one ranked yet",
  "board.emptyText": "Complete a mission or join a contest to appear here.",
  "board.you": "Your position",
  "board.notRanked": "Not ranked yet",
  "board.notRankedText": "Earn XP or trade in this contest to get a rank.",
  "board.belowTop": "You are outside the top shown above. Keep going.",
  "board.privacyTitle": "Your privacy",
  "board.privacyText": "Other traders only see your nickname, rank, score and level, never your name, email or account number. Change your nickname or hide it from the public website in your profile.",

  // Nickname (profile)
  "nick.title": "Leaderboard nickname",
  "nick.subtitle": "What other traders see instead of your name",
  "nick.label": "Nickname",
  "nick.hint": "3 to 20 letters, digits, _ - or .",
  "nick.save": "Save nickname",
  "nick.suggest": "Suggest one",
  "nick.saved": "Nickname saved",
  "nick.error": "Couldn't save your nickname",
  "nick.publicLabel": "Show me on the public website",
  "nick.publicHint": "Your nickname, rank and level on public leaderboards. Off keeps you on the in-app boards only.",
  "nick.publicOn": "You are shown on public leaderboards",
  "nick.publicOff": "You are hidden from public leaderboards",

  // Kalks Trader rank widget
  "trader.title": "Contest rank",
  "trader.rank": "#{rank} of {ranked}",
  "trader.unranked": "Not ranked yet",
  "trader.needsTrades": { one: "{count} more trade to rank", other: "{count} more trades to rank" },
  "trader.ends": "Ends {time}",
  "trader.level": "Level {level} · {name}",
  "trader.open": "Open leaderboard",
};

export default challenges;
