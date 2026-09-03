/**
 * A real OpenVole hub plus one member ("peer-b"), under test control.
 *
 *   tsx hub.ts <hubPort> <memberPort> <dir>
 *
 * stdout: one JSON object per line — `{ev:"ready", ...}` once peer-b is on the roster, then
 * whatever peer-b's bus reports (chat, request, accepted, queued, flushed, pending) and the
 * replies to commands. stdin: one JSON command per line:
 *
 *   {cmd:"send", to, text}   {cmd:"connect", to, note?}   {cmd:"approve", to}
 *   {cmd:"stop-member"}      {cmd:"start-member"}          {cmd:"history", peer}
 *   {cmd:"outbox"}           {cmd:"pending"}               {cmd:"exit"}
 *
 * The hub is a public hub (self-join) with the relay on; peer-b accepts relayed chat from any
 * member, the community-hub shape. OpenVole is imported from beside this repo.
 */
import * as fs from 'node:fs/promises'
import * as path from 'node:path'
import * as readline from 'node:readline'
import { createMessageBus } from '../../openvole/src/core/src/core/bus.ts'
import { VoleNetManager } from '../../openvole/src/core/src/net/index.ts'
import { generateKeyPair, trustPeer } from '../../openvole/src/core/src/net/keys.ts'

const [hubPortArg, memberPortArg, dir] = process.argv.slice(2)
const HUB = Number(hubPortArg)
const MEMBER = Number(memberPortArg)
const hubUrl = `http://127.0.0.1:${HUB}`
const out = (v: Record<string, unknown>) => process.stdout.write(`${JSON.stringify(v)}\n`)

const rootHub = path.join(dir, 'hub')
const rootB = path.join(dir, 'peer-b')
let hub: VoleNetManager
let member: VoleNetManager | null = null
let memberId = ''
let hubKey = ''

const memberCfg = () => ({
	enabled: true,
	instanceName: 'peer-b',
	role: 'peer' as const,
	port: MEMBER,
	peers: [{ url: hubUrl, trust: 'full' as const }],
	relay: { acceptFrom: '*' as const },
})

async function startMember(): Promise<void> {
	const bus = createMessageBus()
	const forward = (ev: string) =>
		bus.on(ev as never, ((data: unknown) => out({ ev, ...(data as Record<string, unknown>) })) as never)
	forward('volenet:chat')
	forward('volenet:relay:request')
	forward('volenet:relay:accepted')
	forward('volenet:relay:denied')
	forward('volenet:chat:queued')
	forward('volenet:chat:flushed')
	forward('volenet:chat:pending')
	member = new VoleNetManager(memberCfg(), rootB)
	await member.start(undefined, bus)
	out({ ev: 'member-started' })
}

async function main(): Promise<void> {
	for (const d of [rootHub, rootB]) await fs.mkdir(path.join(d, '.openvole/net'), { recursive: true })
	const kh = await generateKeyPair(path.join(rootHub, '.openvole/net'), 'harness-hub')
	const kb = await generateKeyPair(path.join(rootB, '.openvole/net'), 'peer-b')
	await trustPeer(path.join(rootHub, '.openvole/net'), kb.publicKeyString)
	await trustPeer(path.join(rootB, '.openvole/net'), kh.publicKeyString)
	memberId = kb.instanceId
	hubKey = kh.publicKeyString

	hub = new VoleNetManager(
		{
			enabled: true,
			instanceName: 'harness-hub',
			role: 'coordinator',
			port: HUB,
			hostname: '127.0.0.1',
			relay: { enabled: true },
			publicJoin: { enabled: true, trustLevel: 'tool', maxPeers: 50, ratePerMinute: 60 },
			publishNames: true,
		},
		rootHub,
	)
	await hub.start()
	await startMember()

	// Ready once the hub lists peer-b as connected.
	const t0 = Date.now()
	while (Date.now() - t0 < 30000) {
		// biome-ignore lint/suspicious/noExplicitAny: harness reads internals
		if ((hub as any).transport.isPeerConnected(memberId)) break
		await new Promise((r) => setTimeout(r, 150))
	}
	out({ ev: 'ready', hubUrl, hubKey, hubId: kh.instanceId, memberId, memberName: 'peer-b' })

	const rl = readline.createInterface({ input: process.stdin })
	for await (const line of rl) {
		const trimmed = line.trim()
		if (!trimmed) continue
		let c: Record<string, string>
		try {
			c = JSON.parse(trimmed)
		} catch {
			out({ ev: 'error', error: `bad command: ${trimmed}` })
			continue
		}
		try {
			switch (c.cmd) {
				case 'send':
					out({ ev: 'sent', ...(await member!.sendChat(c.to, c.text)) })
					break
				case 'connect':
					out({ ev: 'connect-result', ...(await member!.requestRelayConnect(c.to, c.note)) })
					break
				case 'approve':
					out({ ev: 'approve-result', ...(await member!.approveRelayConnect(c.to)) })
					break
				case 'stop-member':
					await member?.stop()
					member = null
					out({ ev: 'member-stopped' })
					break
				case 'start-member':
					await startMember()
					break
				case 'history':
					out({ ev: 'history', history: await member!.getChatHistory(c.peer) })
					break
				case 'outbox':
					out({ ev: 'outbox', outbox: member!.getChatOutbox() })
					break
				case 'pending':
					out({ ev: 'pending-list', pending: member!.getChatPending() })
					break
				case 'exit':
					await member?.stop()
					await hub.stop()
					out({ ev: 'bye' })
					process.exit(0)
				default:
					out({ ev: 'error', error: `unknown command: ${c.cmd}` })
			}
		} catch (e) {
			out({ ev: 'error', error: e instanceof Error ? e.message : String(e) })
		}
	}
}

main().catch((e) => {
	process.stderr.write(`${e instanceof Error ? e.stack ?? e.message : String(e)}\n`)
	process.exit(1)
})
