/**
 * Interop harness: the real OpenVole implementation, one primitive per command.
 *
 * Imports core's TypeScript sources directly (OpenVole checked out beside this repo) and is
 * driven by the Rust crate's integration tests, so a Rust signature is verified by the very
 * code a hub runs, and a hub-sealed envelope is opened by Rust — both directions, every
 * primitive. Runs under tsx. JSON in via argv, JSON out on stdout.
 *
 *   keygen <netDir> <name>                       → { keyString, instanceId, xPublicKey, mlkemPublicKey }
 *   sign   <netDir> <type> <to> <payloadJson>    → the signed message
 *   verify <keyString> <messageJson>             → { valid, error? }
 *   seal   <xPub> <mlkemPub|-> <aad> <plaintext> → the sealed box
 *   unseal <netDir> <aad> <boxJson>              → { plaintext } or { plaintext: null }
 */
import * as fs from 'node:fs/promises'
import { generateKeyPair, loadKeyPair, parsePublicKey } from '../../openvole/src/core/src/net/keys.ts'
import { createMessage, verifyMessage } from '../../openvole/src/core/src/net/protocol.ts'
import { seal, unseal } from '../../openvole/src/core/src/net/seal.ts'

const [cmd, ...a] = process.argv.slice(2)
const out = (v: unknown) => process.stdout.write(`${JSON.stringify(v)}\n`)

async function main(): Promise<void> {
	switch (cmd) {
		case 'keygen': {
			const [netDir, name] = a
			await fs.mkdir(netDir, { recursive: true })
			const kp = (await loadKeyPair(netDir)) ?? (await generateKeyPair(netDir, name))
			out({
				keyString: kp.publicKeyString,
				instanceId: kp.instanceId,
				xPublicKey: kp.xPublicKeyB64,
				mlkemPublicKey: kp.mlkemPublicKeyB64,
			})
			return
		}
		case 'sign': {
			const [netDir, type, to, payloadJson] = a
			const kp = await loadKeyPair(netDir)
			if (!kp) throw new Error(`no keypair in ${netDir}`)
			const payload = payloadJson === undefined ? undefined : JSON.parse(payloadJson)
			out(createMessage(type as never, kp.instanceId, to, payload, kp.privateKey, kp.pqPrivateKey))
			return
		}
		case 'verify': {
			const [keyString, messageJson] = a
			const peer = parsePublicKey(keyString)
			if (!peer) throw new Error('bad key string')
			out(verifyMessage(JSON.parse(messageJson), peer.publicKey, peer.pqPublicKey))
			return
		}
		case 'seal': {
			const [xPub, mlkemPub, aad, plaintext] = a
			const box = seal(xPub, Buffer.from(plaintext, 'utf8'), aad, mlkemPub === '-' ? undefined : mlkemPub)
			out(box)
			return
		}
		case 'unseal': {
			const [netDir, aad, boxJson] = a
			const kp = await loadKeyPair(netDir)
			if (!kp?.xPrivateKey) throw new Error(`no keypair in ${netDir}`)
			const plain = unseal(kp.xPrivateKey, JSON.parse(boxJson), aad, kp.mlkemPrivateKey)
			out({ plaintext: plain ? plain.toString('utf8') : null })
			return
		}
		default:
			throw new Error(`unknown command: ${cmd}`)
	}
}

main().catch((e) => {
	process.stderr.write(`${e instanceof Error ? e.stack ?? e.message : String(e)}\n`)
	process.exit(1)
})
