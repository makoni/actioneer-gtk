# Actioneer 1.4.0

Actioneer now remembers what it last saw, so your repositories are there the
moment it starts and stay readable when GitHub cannot be reached.

## What's changed

- **Instant start.** The repository list, the repository you had open, and
  its workflows with the last result of each appear at once, from the data
  Actioneer saw last time. Fresh data replaces them as soon as GitHub answers.
- **Working offline.** When GitHub cannot be reached, Actioneer says
  "Offline" where the rate limit is normally shown, and each repository says
  how old the data on screen is. A failed refresh keeps what you were reading
  instead of emptying the page. Once the connection is back, Actioneer
  reloads by itself.
- **Fewer requests.** Opening a finished run again shows its jobs without
  asking GitHub, which leaves more of your rate limit for everything else.

## Before you update

Your settings, favorites and sign-in carry over. The saved data lives in your
cache folder and belongs to the account you are signed in with: signing out
deletes it.
