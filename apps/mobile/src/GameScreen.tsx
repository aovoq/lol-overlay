import type { MobileGame, PairingLink } from "@lol-overlay/protocol";
import { useKeepAwake } from "expo-keep-awake";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  ActivityIndicator,
  Alert,
  Image,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  useWindowDimensions,
  Vibration,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { type ConnectionState, useDataDragonVersion, useRelay } from "./useRelay";

const DD = "https://ddragon.leagueoflegends.com";

function formatTime(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

function keyedItems(items: number[]): { id: number; key: string }[] {
  const occurrences = new Map<number, number>();
  return items.map((id) => {
    const occurrence = (occurrences.get(id) ?? 0) + 1;
    occurrences.set(id, occurrence);
    return { id, key: `${id}-${occurrence}` };
  });
}

function ChampionIcon({
  rawName,
  version,
  compact = false,
}: {
  rawName: string;
  version: string;
  compact?: boolean;
}) {
  if (!rawName || !version) {
    return <View style={[styles.championFallback, compact && styles.championIconCompact]} />;
  }
  return (
    <Image
      source={{ uri: `${DD}/cdn/${version}/img/champion/${rawName}.png` }}
      style={[styles.championIcon, compact && styles.championIconCompact]}
    />
  );
}

function ItemIcon({
  id,
  version,
  compact = false,
}: {
  id: number;
  version: string;
  compact?: boolean;
}) {
  return (
    <Image
      source={{ uri: `${DD}/cdn/${version}/img/item/${id}.png` }}
      style={[styles.itemIcon, compact && styles.itemIconCompact]}
    />
  );
}

function StatusLine({ state, stale }: { state: ConnectionState; stale: boolean }) {
  const label = stale
    ? "更新停止"
    : state === "live"
      ? "LIVE"
      : state === "waiting"
        ? "試合開始待ち"
        : state === "reconnecting"
          ? "再接続中"
          : "接続中";
  return (
    <View style={styles.statusLine}>
      <View style={[styles.statusDot, (state !== "live" || stale) && styles.statusDotWaiting]} />
      <Text style={styles.statusText}>{label}</Text>
    </View>
  );
}

function HeroBand({
  game,
  gameTime,
  version,
  compact,
}: {
  game: MobileGame;
  gameTime: number;
  version: string;
  compact: boolean;
}) {
  return (
    <View style={[styles.heroBand, compact && styles.heroBandCompact]}>
      <ChampionIcon rawName={game.selfRawName} version={version} compact={compact} />
      <View style={styles.heroIdentity}>
        <Text style={[styles.selfChampion, compact && styles.selfChampionCompact]}>
          {game.selfChampion || game.selfRawName}
        </Text>
        <Text style={styles.selfMeta}>{game.selfPosition || game.gameMode}</Text>
      </View>
      <Text style={[styles.gameTime, compact && styles.gameTimeCompact]}>
        {formatTime(gameTime)}
      </Text>
    </View>
  );
}

function EnemyLoadout({
  game,
  version,
  compact,
}: {
  game: MobileGame;
  version: string;
  compact: boolean;
}) {
  return (
    <View style={[styles.section, compact && styles.sectionCompact]}>
      <Text style={styles.sectionLabel}>ENEMY LOADOUT</Text>
      <View style={styles.enemyList}>
        {game.enemies.map((enemy) => (
          <View
            style={[styles.playerRow, compact && styles.playerRowCompact]}
            key={`${enemy.rawName}-${enemy.position}`}
          >
            <ChampionIcon rawName={enemy.rawName} version={version} compact={compact} />
            <View style={[styles.playerIdentity, compact && styles.playerIdentityCompact]}>
              <Text numberOfLines={1} style={styles.playerName}>
                {enemy.name}
              </Text>
              <Text style={styles.playerRole}>{enemy.position || "UNKNOWN"}</Text>
            </View>
            <View style={styles.itemStrip}>
              {keyedItems(enemy.items.slice(0, 7)).map((item) => (
                <ItemIcon id={item.id} version={version} compact={compact} key={item.key} />
              ))}
            </View>
          </View>
        ))}
      </View>
    </View>
  );
}

function Recommendations({
  game,
  version,
  limit,
  compact,
}: {
  game: MobileGame;
  version: string;
  limit: number;
  compact: boolean;
}) {
  return (
    <View style={[styles.section, compact && styles.sectionCompact]}>
      <Text style={styles.sectionLabel}>NEXT ITEMS</Text>
      <View style={styles.recommendationList}>
        {game.items.length ? (
          game.items.slice(0, limit).map((item, index) => (
            <View
              style={[styles.recommendationRow, compact && styles.recommendationRowCompact]}
              key={item.itemId}
            >
              <ItemIcon id={item.itemId} version={version} compact={compact} />
              <View style={styles.recommendationCopy}>
                <Text numberOfLines={1} style={styles.recommendationName}>
                  {item.name}
                </Text>
                <Text numberOfLines={1} style={styles.recommendationReason}>
                  {item.reason}
                </Text>
              </View>
              <Text style={styles.recommendationRank}>{String(index + 1).padStart(2, "0")}</Text>
            </View>
          ))
        ) : (
          <Text style={styles.emptyText}>推奨アイテムを取得中</Text>
        )}
      </View>
    </View>
  );
}

function ThreatSummary({ game }: { game: MobileGame }) {
  const metrics = [
    ["AD", game.threats.adCount, "#e7a654"],
    ["AP", game.threats.apCount, "#9faafc"],
    ["TANK", game.threats.tankCount, "#92cf8c"],
    ["CC", game.threats.ccHeavy ? "HIGH" : "LOW", game.threats.ccHeavy ? "#ff7183" : "#8e939f"],
  ] as const;
  return (
    <View style={styles.threatGrid}>
      {metrics.map(([label, value, color]) => (
        <View style={styles.threatMetric} key={label}>
          <Text style={styles.threatLabel}>{label}</Text>
          <Text style={[styles.threatValue, { color }]}>{value}</Text>
        </View>
      ))}
    </View>
  );
}

function SkillSummary({ game }: { game: MobileGame }) {
  const skillNames = ["", "Q", "W", "E", "R"];
  const order = game.skillOrder?.maxOrder.map((skill) => skillNames[skill]).filter(Boolean) ?? [];
  return (
    <View style={styles.skillSummary}>
      <Text style={styles.detailLabel}>SKILL PRIORITY</Text>
      <View style={styles.skillOrder}>
        {order.length ? (
          order.map((skill, index) => (
            <View style={styles.skillStep} key={skill}>
              <Text style={styles.skillKey}>{skill}</Text>
              {index < order.length - 1 && <Text style={styles.skillArrow}>›</Text>}
            </View>
          ))
        ) : (
          <Text style={styles.detailMuted}>取得中</Text>
        )}
      </View>
    </View>
  );
}

function OverviewPage({
  game,
  gameTime,
  version,
  landscape,
}: {
  game: MobileGame;
  gameTime: number;
  version: string;
  landscape: boolean;
}) {
  if (landscape) {
    return (
      <View style={styles.landscapeColumns}>
        <View style={styles.landscapePrimary}>
          <HeroBand game={game} gameTime={gameTime} version={version} compact />
          <EnemyLoadout game={game} version={version} compact />
        </View>
        <View style={styles.landscapeSecondary}>
          <Recommendations game={game} version={version} limit={3} compact />
          <View style={styles.landscapeThreats}>
            <ThreatSummary game={game} />
          </View>
        </View>
      </View>
    );
  }

  return (
    <View style={styles.portraitPage}>
      <HeroBand game={game} gameTime={gameTime} version={version} compact={false} />
      <EnemyLoadout game={game} version={version} compact />
      <Recommendations game={game} version={version} limit={2} compact />
    </View>
  );
}

function IntelPage({
  game,
  version,
  landscape,
}: {
  game: MobileGame;
  version: string;
  landscape: boolean;
}) {
  const details = (
    <View style={styles.intelDetails}>
      <View>
        <Text style={styles.sectionLabel}>MATCH INTEL</Text>
        <ThreatSummary game={game} />
      </View>
      <SkillSummary game={game} />
      <View>
        <Text style={styles.detailLabel}>ALLIED TEAM</Text>
        <View style={styles.allyList}>
          {game.allies.map((ally) => (
            <View style={styles.allyChip} key={ally}>
              <Text numberOfLines={1} style={styles.allyName}>
                {ally}
              </Text>
            </View>
          ))}
        </View>
      </View>
    </View>
  );

  return (
    <View style={[styles.intelPage, landscape && styles.landscapeColumns]}>
      <View style={landscape ? styles.landscapePrimary : styles.intelPrimary}>{details}</View>
      <View style={landscape ? styles.landscapeSecondary : styles.intelRecommendations}>
        <Recommendations game={game} version={version} limit={5} compact />
      </View>
    </View>
  );
}

export function GameScreen({
  link,
  onDisconnect,
}: {
  link: PairingLink;
  onDisconnect: () => void;
}) {
  const { state, snapshot, receivedAt, connectedAt, error, respondToReadyCheck } = useRelay(link);
  const version = useDataDragonVersion();
  const [now, setNow] = useState(Date.now());
  const [page, setPage] = useState(0);
  const pager = useRef<ScrollView>(null);
  const { width, height } = useWindowDimensions();
  const previousWidth = useRef(width);
  const readyCheckNotified = useRef(false);
  const [responsePending, setResponsePending] = useState(false);
  const [responseError, setResponseError] = useState("");
  const landscape = width > height;
  useKeepAwake();

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    if (previousWidth.current === width) return;
    previousWidth.current = width;
    setPage(0);
    pager.current?.scrollTo({ x: 0, animated: false });
  }, [width]);

  // The desktop publishes a snapshot every ~2s while it runs, so silence
  // means the producer is gone — even if no snapshot ever arrived (a phone
  // paired to a session whose desktop app was killed sees exactly that).
  const lastAliveAt = Math.max(receivedAt, connectedAt);
  const stale = lastAliveAt > 0 && now - lastAliveAt > 6_000;
  const gameTime = snapshot?.game
    ? snapshot.game.gameTime + Math.max(0, now - receivedAt) / 1000
    : 0;
  const matchmaking = snapshot?.matchmaking;
  const readyCheck = matchmaking?.state === "readyCheck";
  const respond = useCallback(
    async (response: "accept" | "decline") => {
      if (responsePending) return;
      setResponsePending(true);
      setResponseError("");
      try {
        await respondToReadyCheck(response);
      } catch {
        setResponseError("Windowsへ操作を送信できませんでした");
      } finally {
        setResponsePending(false);
      }
    },
    [respondToReadyCheck, responsePending],
  );

  useEffect(() => {
    if (!readyCheck) {
      readyCheckNotified.current = false;
      return;
    }
    if (readyCheckNotified.current) return;
    readyCheckNotified.current = true;
    Vibration.vibrate([0, 300, 150, 300]);
    Alert.alert("マッチが見つかりました", "承諾しますか？", [
      { text: "拒否", style: "destructive", onPress: () => void respond("decline") },
      { text: "承諾", onPress: () => void respond("accept") },
    ]);
  }, [readyCheck, respond]);

  if (!snapshot?.game) {
    return (
      <SafeAreaView style={styles.waitingRoot} edges={["top", "bottom"]}>
        <View style={styles.topBar}>
          <Text style={styles.brandSmall}>LOL SIDEBOARD</Text>
          <StatusLine state={state} stale={stale} />
        </View>
        <View style={styles.waitingContent}>
          {!readyCheck && !stale && <ActivityIndicator color="#ff465d" size="large" />}
          <Text style={styles.waitingTitle}>
            {state === "error"
              ? "接続できません"
              : stale
                ? "Windowsと通信できません"
                : readyCheck
                  ? "マッチが見つかりました"
                  : matchmaking?.state === "searching"
                    ? `検索中 ${formatTime(matchmaking.timeInQueue + Math.max(0, now - receivedAt) / 1000)}`
                    : "試合を待っています"}
          </Text>
          <Text style={styles.waitingCopy}>
            {error ||
              responseError ||
              (stale
                ? "Windows側のデスクトップアプリが起動しているか確認してください。復帰しない場合は接続を解除して再ペアリングしてください。"
                : readyCheck
                  ? matchmaking.playerResponse === "accepted"
                    ? "承諾しました"
                    : matchmaking.playerResponse === "declined"
                      ? "拒否しました"
                      : "時間内に応答してください"
                  : matchmaking?.state === "searching"
                    ? `予想待ち時間 ${formatTime(matchmaking.estimatedQueueTime)}`
                    : "WindowsでLoLの試合が始まると自動で表示します。")}
          </Text>
          {readyCheck && matchmaking.playerResponse === "none" && (
            <View style={styles.readyActions}>
              <Pressable
                disabled={responsePending}
                style={[styles.readyButton, styles.declineButton]}
                onPress={() => void respond("decline")}
              >
                <Text style={styles.declineButtonText}>拒否</Text>
              </Pressable>
              <Pressable
                disabled={responsePending}
                style={[styles.readyButton, styles.acceptButton]}
                onPress={() => void respond("accept")}
              >
                <Text style={styles.acceptButtonText}>承諾</Text>
              </Pressable>
            </View>
          )}
        </View>
        <Pressable
          accessibilityRole="button"
          style={styles.disconnectButton}
          onPress={onDisconnect}
        >
          <Text style={styles.disconnectText}>接続を解除</Text>
        </Pressable>
      </SafeAreaView>
    );
  }

  const game = snapshot.game;
  const showPage = (nextPage: number) => {
    setPage(nextPage);
    pager.current?.scrollTo({ x: width * nextPage, animated: true });
  };

  return (
    <SafeAreaView style={styles.gameRoot} edges={["top", "bottom"]}>
      <View style={[styles.topBar, landscape && styles.topBarCompact]}>
        <Text style={styles.brandSmall}>LOL SIDEBOARD</Text>
        <View style={styles.topBarActions}>
          <StatusLine state={state} stale={stale} />
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="接続を解除"
            style={styles.topDisconnect}
            onPress={onDisconnect}
          >
            <Text style={styles.topDisconnectText}>解除</Text>
          </Pressable>
        </View>
      </View>

      <ScrollView
        ref={pager}
        horizontal
        pagingEnabled
        bounces={false}
        showsHorizontalScrollIndicator={false}
        style={styles.pager}
        onMomentumScrollEnd={(event) =>
          setPage(Math.round(event.nativeEvent.contentOffset.x / Math.max(width, 1)))
        }
      >
        <View style={[styles.pageFrame, { width }]}>
          <OverviewPage game={game} gameTime={gameTime} version={version} landscape={landscape} />
        </View>
        <View style={[styles.pageFrame, { width }]}>
          <IntelPage game={game} version={version} landscape={landscape} />
        </View>
      </ScrollView>

      <View style={[styles.pageTabs, landscape && styles.pageTabsCompact]}>
        {(["OVERVIEW", "INTEL"] as const).map((label, index) => (
          <Pressable
            accessibilityRole="tab"
            accessibilityState={{ selected: page === index }}
            style={[styles.pageTab, page === index && styles.pageTabActive]}
            onPress={() => showPage(index)}
            key={label}
          >
            <View style={[styles.pageDot, page === index && styles.pageDotActive]} />
            <Text style={[styles.pageTabText, page === index && styles.pageTabTextActive]}>
              {label}
            </Text>
          </Pressable>
        ))}
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  sectionLabel: { color: "#ff7183", fontSize: 11, fontWeight: "800", letterSpacing: 0 },
  waitingRoot: { flex: 1, backgroundColor: "#090a0d" },
  topBar: {
    minHeight: 52,
    paddingHorizontal: 18,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    borderBottomWidth: 1,
    borderBottomColor: "#292c34",
  },
  topBarCompact: { minHeight: 40, paddingHorizontal: 14 },
  brandSmall: { color: "#d7d9df", fontSize: 11, fontWeight: "800", letterSpacing: 0 },
  topBarActions: { flexDirection: "row", alignItems: "center", gap: 12 },
  topDisconnect: {
    minHeight: 28,
    paddingHorizontal: 8,
    alignItems: "center",
    justifyContent: "center",
  },
  topDisconnectText: { color: "#9297a3", fontSize: 11, fontWeight: "700" },
  statusLine: { flexDirection: "row", alignItems: "center", gap: 7 },
  statusDot: { width: 7, height: 7, borderRadius: 4, backgroundColor: "#42d392" },
  statusDotWaiting: { backgroundColor: "#f2b84b" },
  statusText: { color: "#a4a8b2", fontSize: 11, fontWeight: "700" },
  waitingContent: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 36,
  },
  waitingTitle: { color: "#f6f7f9", fontSize: 22, fontWeight: "800", marginTop: 20 },
  waitingCopy: {
    color: "#9297a3",
    fontSize: 14,
    lineHeight: 21,
    textAlign: "center",
    marginTop: 8,
  },
  readyActions: { flexDirection: "row", gap: 12, marginTop: 28 },
  readyButton: {
    minWidth: 120,
    minHeight: 48,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 3,
  },
  declineButton: { borderWidth: 1, borderColor: "#ff7183" },
  acceptButton: { backgroundColor: "#42d392" },
  declineButtonText: { color: "#ff7183", fontSize: 15, fontWeight: "800" },
  acceptButtonText: { color: "#090a0d", fontSize: 15, fontWeight: "800" },
  disconnectButton: {
    minHeight: 48,
    margin: 18,
    alignItems: "center",
    justifyContent: "center",
    borderTopWidth: 1,
    borderTopColor: "#292c34",
  },
  disconnectText: { color: "#9297a3", fontSize: 13, fontWeight: "700" },
  gameRoot: { flex: 1, overflow: "hidden", backgroundColor: "#090a0d" },
  pager: { flex: 1 },
  pageFrame: { height: "100%", overflow: "hidden" },
  portraitPage: { flex: 1 },
  heroBand: {
    minHeight: 104,
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 18,
    backgroundColor: "#111318",
    borderBottomWidth: 1,
    borderBottomColor: "#3a2630",
  },
  heroBandCompact: { minHeight: 70, paddingHorizontal: 14 },
  heroIdentity: { flex: 1, marginLeft: 14 },
  selfChampion: { color: "#fff", fontSize: 22, fontWeight: "800" },
  selfChampionCompact: { fontSize: 18 },
  selfMeta: { color: "#ff7183", fontSize: 11, fontWeight: "800", marginTop: 4 },
  gameTime: { color: "#f2b84b", fontSize: 31, fontVariant: ["tabular-nums"], fontWeight: "800" },
  gameTimeCompact: { fontSize: 26 },
  championIcon: { width: 48, height: 48, borderRadius: 3, backgroundColor: "#22252d" },
  championFallback: { width: 48, height: 48, borderRadius: 3, backgroundColor: "#22252d" },
  championIconCompact: { width: 38, height: 38 },
  section: { paddingHorizontal: 18, paddingTop: 22, gap: 9 },
  sectionCompact: { paddingHorizontal: 14, paddingTop: 10, gap: 5 },
  enemyList: { flexShrink: 1 },
  playerRow: {
    minHeight: 62,
    flexDirection: "row",
    alignItems: "center",
    borderBottomWidth: 1,
    borderBottomColor: "#252830",
    paddingVertical: 7,
  },
  playerRowCompact: { minHeight: 44, paddingVertical: 3 },
  playerIdentity: { width: 94, paddingHorizontal: 10 },
  playerIdentityCompact: { width: 82, paddingHorizontal: 8 },
  playerName: { color: "#eef0f4", fontSize: 13, fontWeight: "700" },
  playerRole: { color: "#7f8490", fontSize: 9, marginTop: 3 },
  itemStrip: { flex: 1, flexDirection: "row", justifyContent: "flex-end", gap: 3, minHeight: 29 },
  itemIcon: { width: 29, height: 29, borderRadius: 2, backgroundColor: "#22252d" },
  itemIconCompact: { width: 24, height: 24 },
  recommendationList: { gap: 6 },
  recommendationRow: {
    minHeight: 58,
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#111318",
    borderLeftWidth: 2,
    borderLeftColor: "#4f5562",
    padding: 10,
    borderRadius: 3,
  },
  recommendationRowCompact: { minHeight: 46, padding: 7 },
  recommendationCopy: { flex: 1, paddingHorizontal: 11 },
  recommendationName: { color: "#f6f7f9", fontSize: 13, fontWeight: "800" },
  recommendationReason: { color: "#8e939f", fontSize: 10, lineHeight: 14, marginTop: 2 },
  recommendationRank: {
    color: "#ff7183",
    fontSize: 13,
    fontWeight: "800",
    fontVariant: ["tabular-nums"],
  },
  emptyText: { color: "#7f8490", fontSize: 13, paddingVertical: 20 },
  landscapeColumns: { flex: 1, flexDirection: "row", overflow: "hidden" },
  landscapePrimary: { width: "56%", minWidth: 0 },
  landscapeSecondary: {
    flex: 1,
    minWidth: 0,
    borderLeftWidth: 1,
    borderLeftColor: "#292c34",
  },
  landscapeThreats: { paddingHorizontal: 14, paddingTop: 12 },
  intelPage: { flex: 1, overflow: "hidden" },
  intelPrimary: { padding: 18, paddingBottom: 4 },
  intelRecommendations: { flex: 1 },
  intelDetails: { gap: 18 },
  threatGrid: {
    height: 66,
    flexDirection: "row",
    marginTop: 8,
    borderWidth: 1,
    borderColor: "#2b2e36",
    borderRadius: 3,
    overflow: "hidden",
  },
  threatMetric: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    borderRightWidth: 1,
    borderRightColor: "#2b2e36",
    backgroundColor: "#111318",
  },
  threatLabel: { color: "#777d89", fontSize: 9, fontWeight: "800" },
  threatValue: { marginTop: 3, fontSize: 19, fontWeight: "800", fontVariant: ["tabular-nums"] },
  skillSummary: {
    minHeight: 72,
    padding: 12,
    borderLeftWidth: 2,
    borderLeftColor: "#ff465d",
    backgroundColor: "#111318",
  },
  detailLabel: { color: "#8e939f", fontSize: 9, fontWeight: "800" },
  detailMuted: { color: "#777d89", fontSize: 12 },
  skillOrder: { flexDirection: "row", alignItems: "center", marginTop: 8 },
  skillStep: { flexDirection: "row", alignItems: "center" },
  skillKey: {
    width: 32,
    height: 28,
    textAlign: "center",
    textAlignVertical: "center",
    color: "#090a0d",
    backgroundColor: "#f2b84b",
    borderRadius: 3,
    fontSize: 13,
    fontWeight: "900",
    lineHeight: 28,
  },
  skillArrow: { paddingHorizontal: 8, color: "#777d89", fontSize: 20 },
  allyList: { flexDirection: "row", flexWrap: "wrap", gap: 6, marginTop: 8 },
  allyChip: {
    minWidth: 70,
    maxWidth: 110,
    paddingHorizontal: 10,
    paddingVertical: 7,
    borderWidth: 1,
    borderColor: "#343741",
    borderRadius: 3,
    backgroundColor: "#111318",
  },
  allyName: { color: "#d8dae0", fontSize: 11, fontWeight: "700" },
  pageTabs: {
    height: 44,
    flexDirection: "row",
    justifyContent: "center",
    borderTopWidth: 1,
    borderTopColor: "#292c34",
    backgroundColor: "#0c0d11",
  },
  pageTabsCompact: { height: 32 },
  pageTab: {
    minWidth: 112,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 7,
    borderTopWidth: 2,
    borderTopColor: "transparent",
  },
  pageTabActive: { borderTopColor: "#ff465d" },
  pageDot: { width: 5, height: 5, borderRadius: 3, backgroundColor: "#4d515c" },
  pageDotActive: { backgroundColor: "#ff465d" },
  pageTabText: { color: "#666b76", fontSize: 9, fontWeight: "800" },
  pageTabTextActive: { color: "#d9dbe1" },
});
