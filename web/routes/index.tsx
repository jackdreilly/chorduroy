import { asset, Head } from "$fresh/runtime.ts";
import QChart from "../islands/QChart.tsx";

export default function Home() {
  return (
    <html>
      <Head>
        <title>Fresh App</title>
      </Head>
      <body class="p-2">
          <div class="flex items-center p-2">
            <img
              src={asset("/logo.webp")}
              class="w-8 h-8 m-2"
              alt="the fresh logo: a sliced lemon dripping with juice"
            />
            <h1 class="font-bold">Chorduroy</h1>
          </div>
          <QChart />
      </body>
    </html>
  );
}
