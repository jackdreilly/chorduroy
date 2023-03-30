import { asset, Head } from "$fresh/runtime.ts";
import QChart from "../islands/QChart.tsx";

export default function Home() {
  return (
    <html>
      <Head>
        <title>Fresh App</title>
      </Head>
      <body class="w-full m-10">
        <div class="max-w-lg m-auto">
          <div class="flex items-center m-4">
            <img
              src={asset("/logo.webp")}
              class="w-16 h-16 m-2"
              alt="the fresh logo: a sliced lemon dripping with juice"
            />
            <h1 class="text-3xl font-bold">Chorduroy</h1>
          </div>
          <QChart />
        </div>
      </body>
    </html>
  );
}
