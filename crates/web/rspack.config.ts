import { defineConfig } from "@rspack/cli";
import { rspack } from "@rspack/core";

export default defineConfig((env) => {
	const serve = Boolean(env.RSPACK_SERVE);

	return {
		context: import.meta.dirname,
		mode: serve ? "development" : "production",
		entry: "./glue/index.ts",
		devtool: serve ? "cheap-module-source-map" : false,
		// The WebAssembly is far past its size warnings in every build.
		performance: false,
		output: {
			module: true,
			// Named by content for good caching once built. Dev names them by chunk:
			// a hot update fetches the stylesheet again by the name the page has,
			// which a rebuild with content names would have gone from.
			filename: serve ? "[name].js" : "[contenthash].js",
			chunkFilename: serve ? "[name].js" : "[contenthash].js",
			cssFilename: serve ? "[name].css" : "[contenthash].css",
			// The page's WebAssembly sits at the root, beside web_bg.debug.wasm, which the
			// `--debug-info-url` that package.nix gives wasm-bindgen in dev names relative to it.
			assetModuleFilename: "[contenthash][ext]",
			clean: true,
		},
		resolve: {
			extensions: [".ts", ".js"],
			tsConfig: `${import.meta.dirname}/tsconfig.json`,
		},
		module: {
			rules: [
				{
					test: /\.ts$/,
					loader: "builtin:swc-loader",
					options: { jsc: { parser: { syntax: "typescript" }, target: "es2022" } },
					type: "javascript/auto",
				},
				{ test: /\.css$/, type: "css" },
				{ test: /\.woff2$/, type: "asset/resource" },
			],
		},
		plugins: [new rspack.HtmlRspackPlugin({ template: "./page.html", scriptLoading: "module" })],
		devServer: {
			// The bundle already holds what the page imports from generated/. Serving the
			// folder adds web_bg.debug.wasm, which DevTools fetches by name.
			static: { directory: `${import.meta.dirname}/generated`, watch: false },
			proxy: env.proxy ? [{ context: ["/rtc"], target: env.proxy }] : [],
			allowedHosts: "all",
		},
	};
});
