import type { PairingLink } from "@lol-overlay/protocol";
import { parsePairingLink } from "@lol-overlay/protocol";
import { CameraView, useCameraPermissions } from "expo-camera";
import { useCallback, useRef, useState } from "react";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

export function PairingScreen({ onPair }: { onPair: (link: PairingLink) => void }) {
  const [permission, requestPermission] = useCameraPermissions();
  const [scanning, setScanning] = useState(false);
  const [manualValue, setManualValue] = useState("");
  const [error, setError] = useState("");
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
        <Text style={styles.pairingCopy}>Windowsアプリに表示されたQRコードを読み取ります。</Text>
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
        <Text style={styles.sectionLabel}>MANUAL CONNECTION</Text>
        <TextInput
          autoCapitalize="none"
          autoCorrect={false}
          multiline
          placeholder="接続リンクを貼り付け"
          placeholderTextColor="#747985"
          style={styles.input}
          value={manualValue}
          onChangeText={setManualValue}
        />
        <Pressable
          accessibilityRole="button"
          disabled={!manualValue.trim()}
          style={[styles.secondaryButton, !manualValue.trim() && styles.disabledButton]}
          onPress={() => accept(manualValue)}
        >
          <Text style={styles.secondaryButtonText}>接続</Text>
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
    minHeight: 72,
    borderWidth: 1,
    borderColor: "#343741",
    borderRadius: 4,
    color: "#f6f7f9",
    padding: 12,
    textAlignVertical: "top",
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
