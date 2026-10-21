// Written by scripts/guide.sh from docs/GUIDE.md: edit that, then run make fmt.
let guideText = #"""
    # How it works

    ## Queues and playlists

    A **queue** is what is playing now. Playing an album, a playlist or a
    search makes a new queue, so the one you had is still there. Each queue
    remembers its song and where you were in it. There are at most 20: making
    a 21st removes the first one in the list.

    A **playlist** is kept until you delete it. Changing a queue never changes
    a playlist.

    ## What follows you

    Queues, playlists, favourites, Listen Later, play counts, where you are
    in long files and the **Playback** settings are the same on every device
    you sign in on. A change made offline reaches the others when you are
    back online.

    Everything else in Settings belongs to the device it is set on: the look,
    downloads, the song cache, songs on this phone and the everyday play
    speed.

    ## Appearance

    ### Mini player

    The mini player shows on every tab but Now playing.

    - **Bar:** a strip along the bottom.
    - **Floating:** a small pill you can drag anywhere. Tap its cover to play
      or pause; press and hold to open Now playing.
    - **None:** nothing. Playing from a list opens Now playing.

    ## Playback

    ### Previous

    **Restart song first** goes back to the start of a song played past its
    first few seconds; a second press goes to the previous song. **Previous
    song** always goes to the previous one.

    ### Remember position

    Audiobooks, podcasts and mixes at least this long carry on where you left
    them, on any device, however much else you played in between. Files in
    the Podcasts and Audiobooks folders always do.

    ### When played again

    What happens when you play a long file you have started before:
    **Resume** from where you were, **Ask** each time, or **Start over**.

    ### Previous button

    In a long file, Previous never restarts it, because that would lose your
    place. Earbuds also miscount taps.

    - **Previous file:** goes to the previous file. Your place here is kept
      for when you come back.
    - **Seek back (headset):** headsets and the lock screen seek back
      instead. On the screen, Previous still goes to the previous file.
    - **Ignore headset:** headsets and the lock screen do nothing.
    - **Same as songs:** works as it does for songs.

    ### Next button

    In a long file, **Seek forward (headset)** makes headsets and the lock
    screen seek forward instead of going to the next file. On the screen,
    Next still goes to the next file.

    ### Seek back and forward

    The buttons beside Play on Now playing, in the notification and on the
    lock screen, shown for long files.

    ### Rewind on resume

    When you go on with a long file after a pause, it starts a few seconds
    before where it stopped, so a sentence cut off is heard again. **After
    pauses of** sets how long a pause has to be.

    ### Listen Later

    **Remove when finished** takes a file off Listen Later once you have heard
    it as far as **Finished at**.

    ## Sleep timer

    Set from the menu on Now playing. It pauses after a time, after a number
    of songs, or at the end of the queue. By time, the music fades out and
    pauses on time; with **Let the song finish**, it pauses where the song
    playing then ends.

    ## Songs on this phone

    On Android, Dhun can show the songs already on your phone beside the
    server's. They are marked with a phone and are never sent to the server.
    Files shorter than **Skip files shorter than**, such as ringtones and
    voice notes, are left out, and so are excluded folders.

    ## Downloads and the cache

    ### Downloads

    Downloaded songs stay on this device until you remove them, and play
    without a connection. At the **Storage limit**, downloads stop. Nothing is
    deleted to make room.

    ### Song cache

    Songs you play, and the next ones in the queue (10, or 2 on mobile data
    or a phone's hotspot), are kept so they play without waiting. At the
    limit, the songs played longest ago make room. A downloaded song is never
    in the cache as well. The web keeps neither downloads nor a cache.

    A long file, an episode, a book or a mix, is not kept whole: **Cache
    ahead**, under Long files on the Playback page, says how many minutes past
    where you are are fetched, 30 unless you change it, and ten minutes behind
    are kept. The window goes when you leave the file and never fills the cache
    in the music's place. To take a book on a trip, download it.

    ## Reset to defaults

    Each settings page can be put back as it was when Dhun was installed.
    Resetting Playback changes it on all your devices. Nothing you downloaded
    is deleted.
    """#
