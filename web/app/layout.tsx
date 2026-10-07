import type { ReactNode } from "react";

export const metadata = {
  title: "Forge Explorer",
  description: "Onchain version control on Solana",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body
        style={{
          fontFamily: "system-ui, -apple-system, sans-serif",
          margin: "2rem auto",
          maxWidth: "60rem",
          background: "#0b0e14",
          color: "#e6e6e6",
        }}
      >
        <header style={{ borderBottom: "1px solid #26303f", paddingBottom: "1rem" }}>
          <h1 style={{ margin: 0 }}>Forge Explorer</h1>
          <p style={{ color: "#8b98a9", margin: "0.25rem 0 0" }}>
            onchain version control on Solana
          </p>
        </header>
        <main style={{ marginTop: "1.5rem" }}>{children}</main>
      </body>
    </html>
  );
}
