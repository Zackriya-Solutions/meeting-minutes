import './globals.css'
import { Source_Sans_3 } from 'next/font/google'
import Sidebar from '@/components/Sidebar'
import { SidebarProvider } from '@/components/Sidebar/SidebarProvider'
import MainContent from '@/components/MainContent'
import MainNav from '@/components/MainNav'

const sourceSans3 = Source_Sans_3({ 
  subsets: ['latin'],
  weight: ['400', '500', '600', '700'],
  variable: '--font-source-sans-3',
})

export { metadata } from './metadata'

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <meta charSet="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="description" content="Meeting Dashboard" />
        <title>Meeting Dashboard</title>
      </head>
      <body className={`${sourceSans3.variable} font-sans`}>
        <SidebarProvider>
          <div className="flex">
            <Sidebar />
            <div className="flex flex-col w-full min-h-screen">
              <MainNav title="Dashboard" />
              <MainContent>{children}</MainContent>
            </div>
          </div>
        </SidebarProvider>
      </body>
    </html>
  );
}
