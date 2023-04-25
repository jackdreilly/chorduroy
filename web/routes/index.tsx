import { Head } from "$fresh/runtime.ts";
import QChart from "../islands/QChart.tsx";

export default function Home() {
  return (
    <html>
      <Head>
        <title>Fresh App</title>
      </Head>
      <body class="p-2">
          <QChart />
      </body>
    </html>
  );
}
