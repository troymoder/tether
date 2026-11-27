// These sit apart from index.ts so the page's program can import them without
// the webworker library.

export interface Request {
	token: string;
	offset: number;
	length: number;
}

export type Answer =
	| { size: number; type: string; bytes: Uint8Array<ArrayBufferLike> }
	| { error: string };
