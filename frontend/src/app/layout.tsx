"use client";

import "./globals.css";
import { Source_Sans_3 } from "next/font/google";
import Sidebar from "@/components/Sidebar";
import { SidebarProvider, useSidebar } from "@/components/Sidebar/SidebarProvider";
import MainNav from "@/components/MainNav";
import MainContent from "@/components/MainContent";

const sourceSans3 = Source_Sans_3({ 
  subsets: ["latin"],
  weight: ["400", "500", "600", "700"],
  variable: "--font-source-sans-3",
});

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className={`${sourceSans3.variable} font-sans`}>
        <SidebarProvider>
          <AppContainer>{children}</AppContainer>
        </SidebarProvider>
      </body>
    </html>
  );
}

function AppContainer({ children }: { children: React.ReactNode }) {
  const { isCollapsed } = useSidebar();

  return (
    <div className="flex">
      {/* Fixed sidebar */}
      <Sidebar />

      {/* Main area shifts left/right depending on sidebar width */}
      <div
        className={`
          flex-1 min-h-screen transition-all duration-300
          ${isCollapsed ? "ml-16" : "ml-64"}
        `}
      >
        <MainNav title="Dashboard" />
        <div className="w-full px-6 py-10 bg-white rounded-lg shadow">
          <MainContent>{children}</MainContent>
        </div>
      </div>
    </div>
  );
}
