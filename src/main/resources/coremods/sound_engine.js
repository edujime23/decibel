function initializeCoreMod() {
    print("[Decibel CoreMod] Initializing JavaScript CoreMod with Negative Space Protection...");

    return {
        // CoreMod 1: CLASS-Level OpenAL Obliteration
        'decibel_openal_obliteration': {
            'target': {
                'type': 'CLASS',
                'name': 'com.mojang.blaze3d.audio.Library'
            },
            'transformer': function(classNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var TypeInsnNode = Java.type('org.objectweb.asm.tree.TypeInsnNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');
                var IntInsnNode = Java.type('org.objectweb.asm.tree.IntInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var FieldInsnNode = Java.type('org.objectweb.asm.tree.FieldInsnNode');

                print("[Decibel CoreMod] Executing CLASS-level OpenAL context bypass...");

                // 1. Procedural Name Discovery
                // Rather than hardcoding mapped or SRG obfuscated names, we scan descriptors.
                var channelPoolFields = [];
                var longFields = [];
                for (var i = 0; i < classNode.fields.size(); i++) {
                    var field = classNode.fields.get(i);
                    var desc = field.desc;
                    if (desc.indexOf("ChannelPool") !== -1) {
                        channelPoolFields.push(field);
                    } else if (desc === "J") {
                        longFields.push(field);
                    }
                }

                // 2. Find target methods
                var initMethod = null;
                var cleanupMethod = null;
                for (var i = 0; i < classNode.methods.size(); i++) {
                    var m = classNode.methods.get(i);
                    if (m.name === "init" && m.desc === "(Ljava/lang/String;Z)V") {
                        initMethod = m;
                    } else if (m.name === "cleanup" && m.desc === "()V") {
                        cleanupMethod = m;
                    }
                }

                if (initMethod !== null) {
                    print("[Decibel CoreMod] Stubbing Library.init() procedurally...");

                    // Statically extract the correct name of the CountingChannelPool class from original instructions
                    var countingChannelPoolClass = "com/mojang/blaze3d/audio/Library$CountingChannelPool";
                    for (var j = 0; j < initMethod.instructions.size(); j++) {
                        var insn = initMethod.instructions.get(j);
                        if (insn.getOpcode() === Opcodes.NEW) {
                            countingChannelPoolClass = insn.desc;
                            break;
                        }
                    }

                    var newInstructions = new InsnList();

                    // Instantiate both static and streaming channel pools to prevent NPEs in getDebugString()
                    for (var k = 0; k < channelPoolFields.length; k++) {
                        var poolField = channelPoolFields[k];
                        var limit = (k === 0) ? 30 : 8; // standard vanilla pool sizes

                        newInstructions.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        newInstructions.add(new TypeInsnNode(Opcodes.NEW, countingChannelPoolClass));
                        newInstructions.add(new InsnNode(Opcodes.DUP));
                        newInstructions.add(new IntInsnNode(Opcodes.BIPUSH, limit));
                        newInstructions.add(new MethodInsnNode(
                            Opcodes.INVOKESPECIAL,
                            countingChannelPoolClass,
                            "<init>",
                            "(I)V",
                            false
                        ));
                        newInstructions.add(new FieldInsnNode(
                            Opcodes.PUTFIELD,
                            "com/mojang/blaze3d/audio/Library",
                            poolField.name,
                            poolField.desc,
                            false
                        ));
                    }

                    // Set dummy handles (1L) for currentDevice and context so status checks (e.g. context != 0L) pass
                    for (var l = 0; l < longFields.length; l++) {
                        var longField = longFields[l];
                        newInstructions.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        newInstructions.add(new InsnNode(Opcodes.LCONST_1));
                        newInstructions.add(new FieldInsnNode(
                            Opcodes.PUTFIELD,
                            "com/mojang/blaze3d/audio/Library",
                            longField.name,
                            longField.desc,
                            false
                        ));
                    }

                    newInstructions.add(new InsnNode(Opcodes.RETURN));

                    // FIX: Safely clear out exception handlers and local variables to prevent computeAllFrames from throwing NPEs
                    initMethod.tryCatchBlocks.clear();
                    if (initMethod.localVariables !== null) {
                        initMethod.localVariables.clear();
                    }
                    initMethod.instructions.clear();
                    initMethod.instructions.add(newInstructions);
                }

                if (cleanupMethod !== null) {
                    print("[Decibel CoreMod] Stubbing Library.cleanup() cleanly...");
                    var cleanupInstructions = new InsnList();
                    cleanupInstructions.add(new InsnNode(Opcodes.RETURN));

                    cleanupMethod.tryCatchBlocks.clear();
                    if (cleanupMethod.localVariables !== null) {
                        cleanupMethod.localVariables.clear();
                    }
                    cleanupMethod.instructions.clear();
                    cleanupMethod.instructions.add(cleanupInstructions);
                }

                return classNode;
            }
        },

        // CoreMod 2: SoundEngine Play Interception (Method-Level)
        'decibel_sound_bypass': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'play',
                'methodDesc': '(Lnet/minecraft/client/resources/sounds/SoundInstance;)V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var JumpInsnNode = Java.type('org.objectweb.asm.tree.JumpInsnNode');
                var LabelNode = Java.type('org.objectweb.asm.tree.LabelNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.play()...");

                if (methodNode.desc !== "(Lnet/minecraft/client/resources/sounds/SoundInstance;)V") {
                    print("[Decibel CoreMod] WARNING: SoundEngine.play descriptor mismatch. Skipping override.");
                    return methodNode;
                }

                var instructions = new InsnList();
                instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onPlaySound",
                    "(Lnet/minecraft/client/resources/sounds/SoundInstance;)Z",
                    false
                ));

                var label = new LabelNode();
                // Pass-through Negative Space Check: If onPlaySound returns false (e.g. daemon is booting or dead),
                // it falls back to vanilla context execution, ensuring simple sound compatibility.
                instructions.add(new JumpInsnNode(Opcodes.IFEQ, label));
                instructions.add(new InsnNode(Opcodes.RETURN));
                instructions.add(label);

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },

        'decibel_sound_stop': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'stop',
                'methodDesc': '(Lnet/minecraft/client/resources/sounds/SoundInstance;)V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var JumpInsnNode = Java.type('org.objectweb.asm.tree.JumpInsnNode');
                var LabelNode = Java.type('org.objectweb.asm.tree.LabelNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.stop()...");

                if (methodNode.desc !== "(Lnet/minecraft/client/resources/sounds/SoundInstance;)V") {
                    return methodNode;
                }

                var instructions = new InsnList();
                instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onStopSound",
                    "(Lnet/minecraft/client/resources/sounds/SoundInstance;)Z",
                    false
                ));

                var label = new LabelNode();
                instructions.add(new JumpInsnNode(Opcodes.IFEQ, label));
                instructions.add(new InsnNode(Opcodes.RETURN));
                instructions.add(label);

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },

        'decibel_listener_update': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'updateSource',
                'methodDesc': '(Lnet/minecraft/client/Camera;)V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.updateSource()...");

                if (methodNode.desc !== "(Lnet/minecraft/client/Camera;)V") {
                    return methodNode;
                }

                var instructions = new InsnList();
                instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onUpdateListener",
                    "(Lnet/minecraft/client/Camera;)V",
                    false
                ));

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },

        'decibel_sound_stop_all': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'stopAll',
                'methodDesc': '()V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.stopAll()...");

                var instructions = new InsnList();
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onStopAll",
                    "()V",
                    false
                ));

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },

        'decibel_sound_destroy': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'destroy',
                'methodDesc': '()V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.destroy()...");

                var instructions = new InsnList();
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onStopAll",
                    "()V",
                    false
                ));

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        }
    };
}