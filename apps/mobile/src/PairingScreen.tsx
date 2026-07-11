import type { PairingLink } from "@lol-overlay/protocol";
import { normalizePairingCode, parsePairingLink } from "@lol-overlay/protocol";
import { CameraView, useCameraPermissions } from "expo-camera";
import { useCallback, useRef, useState } from "react";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

export function PairingScreen({ onPair }: { onPair: (link: PairingLink) => void }) {
  const [permission, requestPermission] = useCameraPermissions();
  const [scanning, setScanning] = useState(false);
  const [manualValue, setManualValue] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const scanned = useRef(false);

  const accept = useCallback(
    (value: string) => {
      const parsed = parsePairingLink(value.trim());
      if (!parsed) {
        setError("有効な接続コードではありません");
        return;
      }
      setError("");
      onPair(parsed);
    },
    [onPair],
  );

  const acceptCode = useCallback(async () => {
    const code = normalizePairingCode(manualValue);
    const relayUrl = (process.env.EXPO_PUBLIC_MOBILE_RELAY_URL ?? "").trim().replace(/\/$/, "");
    if (!code) {
      setError("6桁の接続コードを入力してください");
      return;
    }
    if (!relayUrl) {
      setError("接続先Relayが設定されていません");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const response = await fetch(`${relayUrl}/v1/pair`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ code }),
      });
      if (!response.ok) throw new Error("invalid code");
      const value: unknown = await response.json();
      const parsed =
        typeof value === "object" && value !== null && "viewerUrl" in value
          ? parsePairingLink(String(value.viewerUrl))
          : null;
      if (!parsed) throw new Error("invalid response");
      onPair(parsed);
    } catch {
      setError("接続コードが無効か、有効期限が切れています");
    } finally {
      setLoading(false);
    }
  }, [manualValue, onPair]);

  if (scanning && permission?.granted) {
    return (
      <View style={styles.scannerRoot}>
        <CameraView
          style={StyleSheet.absoluteFill}
          barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
          onBarcodeScanned={({ data }) => {
            if (scanned.current) return;
            scanned.current = true;
            accept(data);
          }}
        />
        <SafeAreaView style={styles.scannerOverlay}>
          <Text style={styles.scannerTitle}>QRコードを枠内に合わせる</Text>
          <View style={styles.scanFrame} />
          <Pressable
            accessibilityRole="button"
            style={styles.secondaryButton}
            onPress={() => {
              scanned.current = false;
              setScanning(false);
            }}
          >
            <Text style={styles.secondaryButtonText}>キャンセル</Text>
          </Pressable>
        </SafeAreaView>
      </View>
    );
  }

  return (
    <SafeAreaView style={styles.pairingRoot} edges={["top", "bottom"]}>
      <View style={styles.brandBlock}>
        <Text style={styles.brand}>LOL SIDEBOARD</Text>
        <Text style={styles.pairingTitle}>iPhoneを試合画面に接続</Text>
        <Text style={styles.pairingCopy}>
          WindowsアプリのQRコードを読み取るか、6桁のコードを入力します。
        </Text>
      </View>
      <Pressable
        accessibilityRole="button"
        style={styles.primaryButton}
        onPress={async () => {
          scanned.current = false;
          const result = permission?.granted ? permission : await requestPermission();
          if (result.granted) setScanning(true);
          else setError("設定からカメラへのアクセスを許可してください");
        }}
      >
        <Text style={styles.primaryButtonText}>QRコードを読み取る</Text>
      </Pressable>
      <View style={styles.manualSection}>
        <Text style={styles.sectionLabel}>6-DIGIT CONNECTION CODE</Text>
        <TextInput
          accessibilityLabel="6桁の接続コード"
          keyboardType="number-pad"
          maxLength={6}
          placeholder="000000"
          placeholderTextColor="#747985"
          style={styles.input}
          value={manualValue}
          onChangeText={(value) => setManualValue(value.replace(/\D/g, "").slice(0, 6))}
        />
        <Pressable
          accessibilityRole="button"
          disabled={manualValue.length !== 6 || loading}
          style={[
            styles.secondaryButton,
            (manualValue.length !== 6 || loading) && styles.disabledButton,
          ]}
          onPress={acceptCode}
        >
          <Text style={styles.secondaryButtonText}>{loading ? "接続中…" : "接続"}</Text>
        </Pressable>
        {!!error && <Text style={styles.errorText}>{error}</Text>}
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  pairingRoot: {
    flex: 1,
    backgroundColor: "#090a0d",
    paddingHorizontal: 24,
    justifyContent: "center",
  },
  brandBlock: { borderLeftWidth: 3, borderLeftColor: "#ff465d", paddingLeft: 16, marginBottom: 36 },
  brand: { color: "#ff465d", fontSize: 12, fontWeight: "800" },
  pairingTitle: { color: "#f6f7f9", fontSize: 27, fontWeight: "800", marginTop: 10 },
  pairingCopy: { color: "#a4a8b2", fontSize: 14, lineHeight: 21, marginTop: 8 },
  primaryButton: {
    height: 52,
    backgroundColor: "#ff465d",
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 4,
  },
  primaryButtonText: { color: "#090a0d", fontSize: 15, fontWeight: "800" },
  manualSection: { marginTop: 36, gap: 12 },
  sectionLabel: { color: "#ff7183", fontSize: 11, fontWeight: "800" },
  input: {
    minHeight: 58,
    borderWidth: 1,
    borderColor: "#343741",
    borderRadius: 4,
    color: "#f6f7f9",
    padding: 12,
    textAlign: "center",
    fontSize: 28,
    fontWeight: "800",
    letterSpacing: 10,
    backgroundColor: "#111318",
  },
  secondaryButton: {
    minHeight: 46,
    borderWidth: 1,
    borderColor: "#575c68",
    borderRadius: 4,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#15171d",
    paddingHorizontal: 18,
  },
  secondaryButtonText: { color: "#f6f7f9", fontWeight: "700", fontSize: 14 },
  disabledButton: { opacity: 0.4 },
  errorText: { color: "#ff7183", fontSize: 13, lineHeight: 19 },
  scannerRoot: { flex: 1, backgroundColor: "#000" },
  scannerOverlay: {
    flex: 1,
    alignItems: "center",
    justifyContent: "space-between",
    padding: 28,
    backgroundColor: "rgba(0,0,0,0.32)",
  },
  scannerTitle: { color: "#fff", fontSize: 16, fontWeight: "700", marginTop: 24 },
  scanFrame: { width: 260, height: 260, borderWidth: 2, borderColor: "#ff465d", borderRadius: 4 },
});
